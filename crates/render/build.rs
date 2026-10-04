use wgsl_bindgen::{GlamWgslTypeMap, WgslBindgenOptionBuilder, WgslTypeSerializeStrategy};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    WgslBindgenOptionBuilder::default()
        .workspace_root("shaders")
        .add_entry_point("shaders/mesh.wgsl")
        .add_entry_point("shaders/sprite.wgsl")
        .add_entry_point("shaders/blit.wgsl")
        .add_entry_point("shaders/tonemap.wgsl")
        .add_entry_point("shaders/shadow.wgsl")
        .serialization_strategy(WgslTypeSerializeStrategy::Bytemuck)
        .type_map(GlamWgslTypeMap)
        .output("src/shader_bindings.rs")
        .build()?
        .generate()?;
    Ok(())
}
