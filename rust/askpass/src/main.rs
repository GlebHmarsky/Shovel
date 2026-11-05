use std::io::Write;
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let answer = if args.len() == 1 {
        shovel_core::authentication::client_from_environment(&args[0])
    } else {
        Err(std::io::ErrorKind::InvalidInput.into())
    };
    let code = match answer {
        Ok(answer) => {
            let mut output = std::io::stdout().lock();
            if output
                .write_all(&answer)
                .and_then(|_| output.write_all(b"\n"))
                .and_then(|_| output.flush())
                .is_ok()
            {
                0
            } else {
                1
            }
        }
        Err(_) => 1,
    };
    std::process::exit(code);
}
