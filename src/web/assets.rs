use rust_embed::Embed;

#[derive(Embed)]
#[folder = "frontend/dist/"]
pub struct Assets;
