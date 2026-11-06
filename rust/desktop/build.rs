#[path = "../windows_resource.rs"]
mod resource;
fn main() {
    resource::compile("ShovelRust.exe", "Shovel Rust");
}
