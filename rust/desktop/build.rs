#[path = "../windows_resource.rs"]
mod resource;
fn main() {
    resource::compile("Shovel.exe", "Shovel");
}
