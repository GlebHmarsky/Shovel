use eframe::egui::ColorImage;
use sha2::{Digest, Sha256};
use shovel_core::process::Cancellation;
use std::{
    io::{Cursor, Read},
    time::{Duration, Instant},
};
use url::Url;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum Source {
    Gravatar,
    GitHub(String),
    Gitea { website: Url, repository: String },
}

fn public_http(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https")
        && url.has_host()
        && url.username().is_empty()
        && url.password().is_none()
}

pub(super) fn repository(remote: &str, website: Option<&Url>) -> Option<Source> {
    let repo = if remote.contains("://") {
        Url::parse(remote).ok()?
    } else {
        let (host, path) = remote.split_once(':')?;
        if host.len() <= 1 || host.contains(['/', '\\']) {
            return None;
        }
        Url::parse(&format!("ssh://{host}/{path}")).ok()?
    };
    if !matches!(repo.scheme(), "https" | "http" | "ssh" | "git") {
        return None;
    }
    let host = repo.host_str()?;
    let path = repo.path().trim_matches('/');
    let path = if path.to_ascii_lowercase().ends_with(".git") {
        &path[..path.len() - 4]
    } else {
        path
    };
    let github = host.eq_ignore_ascii_case("github.com");
    let path = if github {
        path
    } else {
        let site = website?;
        if site.host_str()? != host {
            return None;
        }
        let prefix = site.path().trim_start_matches('/');
        path.strip_prefix(prefix)
            .or_else(|| matches!(repo.scheme(), "ssh" | "git").then_some(path))?
    };
    let (owner, name) = path.split_once('/')?;
    if owner.is_empty() || name.is_empty() || name.contains('/') || path.contains(['?', '#']) {
        return None;
    }
    Some(if github {
        Source::GitHub(path.to_owned())
    } else {
        Source::Gitea {
            website: website?.clone(),
            repository: path.to_owned(),
        }
    })
}

pub(super) struct Request {
    pub source: Source,
    pub email: String,
    pub commit: String,
}

enum ResponseError {
    Unavailable,
    Status(u16),
}
type Download = Result<Vec<u8>, ResponseError>;

impl Request {
    fn urls(&self) -> Option<(Url, Option<Url>)> {
        if self.source != Source::Gravatar
            && (!matches!(self.commit.len(), 40 | 64)
                || !self.commit.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return None;
        }
        match &self.source {
            Source::Gravatar => {
                if self.email.is_empty() {
                    return None;
                }
                let hash = Sha256::digest(self.email.trim().to_lowercase().as_bytes());
                Some((
                    Url::parse(&format!(
                        "https://www.gravatar.com/avatar/{hash:x}?s=64&d=404"
                    ))
                    .ok()?,
                    None,
                ))
            }
            Source::GitHub(repo) => Some((
                Url::parse(&format!(
                    "https://api.github.com/repos/{repo}/commits/{}?per_page=1",
                    self.commit
                ))
                .ok()?,
                None,
            )),
            Source::Gitea {
                website,
                repository,
            } => {
                let metadata = website.join(&format!("api/v1/repos/{repository}/git/commits/{}?stat=false&verification=false&files=false", self.commit)).ok()?;
                let mut search = website.join("api/v1/users/search").ok()?;
                search
                    .query_pairs_mut()
                    .append_pair("q", self.email.trim())
                    .append_pair("limit", "10");
                Some((metadata, (!self.email.trim().is_empty()).then_some(search)))
            }
        }
    }
    fn image_bytes(&self, mut fetch: impl FnMut(Url, usize) -> Download) -> Option<Vec<u8>> {
        let (url, search) = self.urls()?;
        let image = if self.source == Source::Gravatar {
            url
        } else {
            let avatar = match fetch(url, 1024 * 1024) {
                Ok(bytes) => {
                    let json: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
                    json.pointer("/author/avatar_url")
                        .and_then(|v| v.as_str())
                        .and_then(|s| Url::parse(s).ok())
                }
                Err(ResponseError::Status(401 | 403 | 404)) => None,
                Err(_) => return None,
            };
            match avatar {
                Some(url) => url,
                None => {
                    let users: serde_json::Value =
                        serde_json::from_slice(&fetch(search?, 256 * 1024).ok()?).ok()?;
                    let user = users.get("data")?.as_array()?.iter().find(|user| {
                        user.get("email")
                            .and_then(|v| v.as_str())
                            .is_some_and(|s| s.eq_ignore_ascii_case(self.email.trim()))
                    })?;
                    Url::parse(user.get("avatar_url")?.as_str()?).ok()?
                }
            }
        };
        if !public_http(&image) {
            return None;
        }
        fetch(image, 2 * 1024 * 1024).ok()
    }
    pub fn load(&self, cancel: &Cancellation) -> Option<ColorImage> {
        let deadline = Instant::now() + Duration::from_secs(8);
        let bytes = self.image_bytes(|url, max| download(url, max, cancel, deadline))?;
        if cancel.is_cancelled() || Instant::now() >= deadline {
            return None;
        }
        decode(&bytes)
    }
}

fn download(mut url: Url, limit: usize, cancel: &Cancellation, deadline: Instant) -> Download {
    use ResponseError::Unavailable;
    for _ in 0..4 {
        if !public_http(&url) || cancel.is_cancelled() {
            return Err(Unavailable);
        }
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or(Unavailable)?;
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .max_redirects(0)
            .http_status_as_error(false)
            .timeout_global(Some(remaining))
            .build()
            .into();
        let mut result = agent
            .get(url.as_str())
            .header("User-Agent", "ShovelRust")
            .call()
            .map_err(|_| Unavailable)?;
        let status = result.status();
        if status.is_redirection() {
            url = url
                .join(
                    result
                        .headers()
                        .get("Location")
                        .and_then(|h| h.to_str().ok())
                        .ok_or(Unavailable)?,
                )
                .map_err(|_| Unavailable)?;
            continue;
        }
        if !status.is_success() {
            return Err(ResponseError::Status(status.as_u16()));
        }
        if result
            .headers()
            .get("Content-Length")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<usize>().ok())
            .is_some_and(|n| n > limit)
        {
            return Err(Unavailable);
        }
        let mut bytes = Vec::new();
        let mut part = [0; 8192];
        let mut reader = result.body_mut().as_reader();
        loop {
            if cancel.is_cancelled() || Instant::now() >= deadline {
                return Err(Unavailable);
            }
            let n = reader.read(&mut part).map_err(|_| Unavailable)?;
            if n == 0 {
                return Ok(bytes);
            }
            if bytes.len() + n > limit {
                return Err(Unavailable);
            }
            bytes.extend_from_slice(&part[..n]);
        }
    }
    Err(Unavailable)
}

fn decode(bytes: &[u8]) -> Option<ColorImage> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(96 * 1024 * 1024);
    reader.limits(limits);
    let thumbnail = reader.decode().ok()?.thumbnail(64, 64).to_rgba8();
    Some(ColorImage::from_rgba_unmultiplied(
        [thumbnail.width() as usize, thumbnail.height() as usize],
        thumbnail.as_raw(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use shovel_core::settings::avatar_website as website;
    use std::{io::Write, net::TcpListener};

    fn png(width: u32, height: u32, format: image::ImageFormat) -> Vec<u8> {
        let mut buffer = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(width, height)
            .write_to(&mut buffer, format)
            .unwrap();
        buffer.into_inner()
    }
    fn request(source: Source) -> Request {
        Request {
            source,
            email: "User+project@example.test".into(),
            commit: "a".repeat(40),
        }
    }

    #[test]
    fn repository_sources_strip_credentials_keep_gitea_subpaths_and_skip_unknown_hosts() {
        let subpath = website(" https://git.example.test:8443/code/ ").unwrap();
        for remote in [
            "https://git.example.test:8443/code/team/app.git",
            "git@git.example.test:team/app.git",
        ] {
            let (metadata, search) = request(repository(remote, Some(&subpath)).unwrap())
                .urls()
                .unwrap();
            assert_eq!(
                metadata.path(),
                format!("/code/api/v1/repos/team/app/git/commits/{}", "a".repeat(40))
            );
            let search = search.unwrap();
            assert_eq!(search.path(), "/code/api/v1/users/search");
            assert_eq!(
                search.query_pairs().next().unwrap().1,
                "User+project@example.test"
            );
        }
        let gh = repository(
            "https://discard:unused@github.com/team/project.GIT?unused=1",
            None,
        )
        .unwrap();
        let (metadata, _) = request(gh).urls().unwrap();
        assert_eq!(metadata.host_str(), Some("api.github.com"));
        assert!(metadata.username().is_empty() && metadata.password().is_none());
        assert!(!metadata.as_str().contains("unused"));
        for raw in [
            "https://other.test/org/repo",
            "https://git.example.test/wrong/repo",
            "C:\\repo",
            "file:///C:/repo",
            "https://github.com/one/two/three",
        ] {
            assert!(repository(raw, Some(&subpath)).is_none(), "{raw}");
        }
        for raw in [
            "https://user@example.test",
            "https://example.test?value",
            "https://example.test#anchor",
            "ftp://example.test",
        ] {
            assert!(website(raw).is_none(), "{raw}");
        }
        let mut bad = request(Source::GitHub("team/repo".into()));
        bad.commit = "HEAD".into();
        assert!(bad.urls().is_none());
    }

    #[test]
    fn gitea_fallback_requires_an_exact_public_email_and_gravatar_hashes_the_normalized_address() {
        let req = request(Source::Gitea {
            website: website("https://git.example.test").unwrap(),
            repository: "team/app".into(),
        });
        let mut order = Vec::new();
        let bytes=req.image_bytes(|url,limit| {
            order.push((url.path().to_owned(),limit));
            match order.len() {
                1 => Err(ResponseError::Status(404)),
                2 => Ok(br#"{"data":[{"email":"other@example.test","avatar_url":"https://git.example.test/wrong"},{"email":"user+project@example.test","avatar_url":"https://git.example.test/picture"}]}"#.to_vec()),
                _ => Ok(vec![1,2,3]),
            }
        });
        assert_eq!(bytes, Some(vec![1, 2, 3]));
        assert_eq!(order[2], ("/picture".into(), 2 * 1024 * 1024));
        assert_eq!(order[1], ("/api/v1/users/search".into(), 256 * 1024));
        assert_eq!(order[0].1, 1024 * 1024);
        assert!(
            req.image_bytes(|_, _| Err(ResponseError::Status(429)))
                .is_none()
        );
        let mut req = request(Source::Gravatar);
        req.email = "  MyEmailAddress@example.com ".into();
        let (url, search) = req.urls().unwrap();
        assert_eq!(
            url.path(),
            format!(
                "/avatar/{:x}",
                Sha256::digest(b"myemailaddress@example.com")
            )
        );
        assert_eq!(url.query(), Some("s=64&d=404"));
        assert!(search.is_none());
    }

    fn serve(status: &str, headers: &str, body: Vec<u8>) -> Url {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
        let header = format!("HTTP/1.1 {status}\r\n{headers}Connection: close\r\n\r\n");
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut request = [0; 4096];
            let _ = stream.read(&mut request);
            let _ = stream.write_all(header.as_bytes());
            let _ = stream.write_all(&body);
        });
        url
    }
    #[test]
    fn real_anonymous_transport_bounds_bytes_redirects_and_decoded_dimensions() {
        let cancel = Cancellation::default();
        let fetch = |url, limit| {
            download(url, limit, &cancel, Instant::now() + Duration::from_secs(3)).ok()
        };
        for format in [image::ImageFormat::Png, image::ImageFormat::Jpeg] {
            let data = png(128, 64, format);
            let last = serve(
                "200 OK",
                &format!("Content-Length: {}\r\n", data.len()),
                data,
            );
            let first = serve("302 Found", &format!("Location: {last}\r\n"), vec![]);
            let image = decode(&fetch(first, 2 * 1024 * 1024).unwrap()).unwrap();
            assert_eq!(image.size, [64, 32]);
        }
        assert!(decode(&png(4097, 1, image::ImageFormat::Png)).is_none());
        assert!(decode(b"not an image").is_none());
        assert!(fetch(serve("200 OK", "Content-Length: 500\r\n", vec![]), 128).is_none());
        assert!(fetch(serve("200 OK", "", vec![0; 129]), 128).is_none());
        assert!(
            fetch(
                serve(
                    "302 Found",
                    "Location: http://unused:example@127.0.0.1/\r\n",
                    vec![]
                ),
                128
            )
            .is_none()
        );
    }

    #[test]
    #[ignore = "contacts the public Shovel release commit on GitHub"]
    fn public_shovel_commit_has_a_decodable_anonymous_author_picture() {
        let req = Request {
            source: Source::GitHub("GlebHmarsky/Shovel".into()),
            email: String::new(),
            commit: "6f3f9098e2bfeeba484da58a3a201f64f0065197".into(),
        };
        let image = req.load(&Cancellation::default()).unwrap();
        assert!(image.width() > 0 && image.height() > 0 && image.width().max(image.height()) == 64);
    }

    #[test]
    fn configured_gitea_fetches_an_exact_public_profile_over_anonymous_http() {
        use std::io::{BufRead, BufReader};
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let site = website(&format!("{origin}/code")).unwrap();
        let req = request(repository("git@127.0.0.1:team/project.git", Some(&site)).unwrap());
        let server = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(8);
            let mut paths = Vec::new();
            while paths.len() < 3 {
                let (mut stream, _) = match listener.accept() {
                    Ok(client) => client,
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        std::thread::sleep(Duration::from_millis(3));
                        continue;
                    }
                    Err(e) => panic!("fixture request missing: {e}"),
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let request: Vec<_> = BufReader::new(&stream)
                    .lines()
                    .map(Result::unwrap)
                    .take_while(|line| !line.is_empty())
                    .take(64)
                    .collect();
                assert!(
                    request
                        .iter()
                        .all(|s| !s.to_ascii_lowercase().starts_with("authorization:")
                            && !s.to_ascii_lowercase().starts_with("cookie:"))
                );
                let path = request[0].split_whitespace().nth(1).unwrap().to_owned();
                let (status,body)=match paths.len() {
                    0=>("404 Not Found",Vec::new()),
                    1=>("200 OK",serde_json::to_vec(&serde_json::json!({"data":[
                        {"email":"unrelated@example.test","avatar_url":format!("{origin}/wrong")},
                        {"email":"user+project@example.test","avatar_url":format!("{origin}/picture")}
                    ]})).unwrap()),
                    _=>("200 OK",png(120,80,image::ImageFormat::Jpeg)),
                };
                paths.push(path);
                write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .unwrap();
                stream.write_all(&body).unwrap();
            }
            paths
        });
        let picture = req.load(&Cancellation::default()).unwrap();
        assert_eq!(picture.width(), 64);
        let paths = server.join().unwrap();
        assert!(paths[0].starts_with("/code/api/v1/repos/team/project/git/commits/"));
        assert!(
            paths[1]
                .starts_with("/code/api/v1/users/search?q=User%2Bproject%40example.test&limit=10")
        );
        assert_eq!(paths[2], "/picture");
    }
}
