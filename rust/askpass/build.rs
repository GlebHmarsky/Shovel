#[path = "../windows_resource.rs"]
mod resource;
fn main() {
    resource::compile("ShovelRustAskPass.exe", "Shovel Rust authentication helper");
}
