use ratatui::style::Color;

/// Elastic brand colorway (sourced from Elastic UI / EUI tokens).
///
/// Reference:
/// - `https://eui.elastic.co/docs/getting-started/theming/tokens/colors`
pub struct ElasticTheme;

impl ElasticTheme {
    // Brand / accents
    pub const PRIMARY: Color = Color::Rgb(0x0B, 0x64, 0xDD); // #0B64DD
    pub const ACCENT: Color = Color::Rgb(0xBC, 0x1E, 0x70); // #BC1E70
    pub const ACCENT_SECONDARY: Color = Color::Rgb(0x00, 0x8B, 0x87); // #008B87

    // Semantic
    pub const SUCCESS: Color = Color::Rgb(0x00, 0x8A, 0x5E); // #008A5E
    pub const WARNING: Color = Color::Rgb(0xFA, 0xCB, 0x3D); // #FACB3D
    pub const DANGER: Color = Color::Rgb(0xC6, 0x1E, 0x25); // #C61E25

    // Neutrals
    pub const SUBTLE: Color = Color::DarkGray;
}

