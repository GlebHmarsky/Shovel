#[path = "../windows_resource.rs"]
mod resource;
fn main() {
    resource::compile("ShovelAskPass.exe", "Shovel authentication helper");
}
