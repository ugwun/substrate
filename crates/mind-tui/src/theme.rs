use ratatui::style::{Color, Modifier, Style};

pub const CANVAS_RGB: (u8, u8, u8) = (0, 31, 39);
pub const PANEL_RGB: (u8, u8, u8) = (5, 45, 54);
pub const RAISED_RGB: (u8, u8, u8) = (7, 54, 66);
pub const CYAN_RGB: (u8, u8, u8) = (42, 161, 152);
pub const CYAN_BRIGHT_RGB: (u8, u8, u8) = (94, 214, 214);
pub const AMBER_RGB: (u8, u8, u8) = (218, 165, 32);
pub const GOLD_RGB: (u8, u8, u8) = (250, 204, 72);
pub const BLUEPRINT_RGB: (u8, u8, u8) = (10, 34, 72);
pub const BLUEPRINT_GRID_RGB: (u8, u8, u8) = (32, 82, 148);
pub const COBALT_BRIGHT_RGB: (u8, u8, u8) = (106, 184, 255);
pub const MAGENTA_RGB: (u8, u8, u8) = (224, 122, 224);

const TEXT_RGB: (u8, u8, u8) = (156, 172, 174);
const TEXT_STRONG_RGB: (u8, u8, u8) = (242, 238, 222);
const MUTED_RGB: (u8, u8, u8) = (108, 132, 142);
const FAINT_RGB: (u8, u8, u8) = (70, 98, 108);
const BORDER_RGB: (u8, u8, u8) = (24, 88, 102);
const SELECTION_RGB: (u8, u8, u8) = (12, 74, 88);
const BLUE_RGB: (u8, u8, u8) = (46, 148, 224);
const ORANGE_RGB: (u8, u8, u8) = (238, 125, 70);
const RED_RGB: (u8, u8, u8) = (245, 110, 105);
const GREEN_RGB: (u8, u8, u8) = (154, 182, 42);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    no_color: bool,
    ansi16: bool,
    pub canvas: Color,
    pub panel: Color,
    pub raised: Color,
    pub blueprint: Color,
    pub blueprint_grid: Color,
    pub text: Color,
    pub text_strong: Color,
    pub muted: Color,
    pub faint: Color,
    pub border: Color,
    pub selection: Color,
    pub cyan: Color,
    pub cyan_bright: Color,
    pub amber: Color,
    pub gold: Color,
    pub blue: Color,
    pub cobalt_bright: Color,
    pub magenta: Color,
    pub warning: Color,
    pub danger: Color,
    pub success: Color,
}

impl Theme {
    pub fn new(no_color: bool) -> Self {
        Self::for_color_mode(no_color, false)
    }

    pub fn for_color_mode(no_color: bool, ansi16: bool) -> Self {
        Self::for_color_tier(no_color, ansi16, false)
    }

    pub fn for_color_tier(no_color: bool, ansi16: bool, ansi256: bool) -> Self {
        if no_color {
            return Self {
                no_color: true,
                ansi16: false,
                canvas: Color::Reset,
                panel: Color::Reset,
                raised: Color::Reset,
                blueprint: Color::Reset,
                blueprint_grid: Color::Reset,
                text: Color::Reset,
                text_strong: Color::Reset,
                muted: Color::Reset,
                faint: Color::Reset,
                border: Color::Reset,
                selection: Color::Reset,
                cyan: Color::Reset,
                cyan_bright: Color::Reset,
                amber: Color::Reset,
                gold: Color::Reset,
                blue: Color::Reset,
                cobalt_bright: Color::Reset,
                magenta: Color::Reset,
                warning: Color::Reset,
                danger: Color::Reset,
                success: Color::Reset,
            };
        }

        if ansi16 {
            return Self {
                no_color: false,
                ansi16: true,
                canvas: Color::Black,
                panel: Color::Black,
                raised: Color::DarkGray,
                blueprint: Color::Blue,
                blueprint_grid: Color::DarkGray,
                text: Color::Gray,
                text_strong: Color::White,
                muted: Color::DarkGray,
                faint: Color::DarkGray,
                border: Color::Cyan,
                selection: Color::Blue,
                cyan: Color::Cyan,
                cyan_bright: Color::LightCyan,
                amber: Color::Yellow,
                gold: Color::LightYellow,
                blue: Color::Blue,
                cobalt_bright: Color::LightBlue,
                magenta: Color::LightMagenta,
                warning: Color::LightYellow,
                danger: Color::LightRed,
                success: Color::LightGreen,
            };
        }

        if ansi256 {
            let idx = |(r, g, b): (u8, u8, u8)| {
                crate::raster::present::to_xterm256_solid(crate::raster::Rgb(r, g, b))
            };
            return Self {
                no_color: false,
                ansi16: false,
                canvas: Color::Indexed(233),
                panel: Color::Indexed(234),
                raised: Color::Indexed(23),
                blueprint: Color::Indexed(17),
                blueprint_grid: Color::Indexed(24),
                text: idx(TEXT_RGB),
                text_strong: idx(TEXT_STRONG_RGB),
                muted: idx(MUTED_RGB),
                faint: idx(FAINT_RGB),
                border: Color::Indexed(24),
                selection: Color::Indexed(23),
                cyan: idx(CYAN_RGB),
                cyan_bright: idx(CYAN_BRIGHT_RGB),
                amber: idx(AMBER_RGB),
                gold: idx(GOLD_RGB),
                blue: idx(BLUE_RGB),
                cobalt_bright: idx(COBALT_BRIGHT_RGB),
                magenta: idx(MAGENTA_RGB),
                warning: idx(ORANGE_RGB),
                danger: idx(RED_RGB),
                success: idx(GREEN_RGB),
            };
        }

        Self {
            no_color: false,
            ansi16: false,
            canvas: rgb(CANVAS_RGB),
            panel: rgb(PANEL_RGB),
            raised: rgb(RAISED_RGB),
            blueprint: rgb(BLUEPRINT_RGB),
            blueprint_grid: rgb(BLUEPRINT_GRID_RGB),
            text: rgb(TEXT_RGB),
            text_strong: rgb(TEXT_STRONG_RGB),
            muted: rgb(MUTED_RGB),
            faint: rgb(FAINT_RGB),
            border: rgb(BORDER_RGB),
            selection: rgb(SELECTION_RGB),
            cyan: rgb(CYAN_RGB),
            cyan_bright: rgb(CYAN_BRIGHT_RGB),
            amber: rgb(AMBER_RGB),
            gold: rgb(GOLD_RGB),
            blue: rgb(BLUE_RGB),
            cobalt_bright: rgb(COBALT_BRIGHT_RGB),
            magenta: rgb(MAGENTA_RGB),
            warning: rgb(ORANGE_RGB),
            danger: rgb(RED_RGB),
            success: rgb(GREEN_RGB),
        }
    }

    pub fn blueprint_style(self) -> Style {
        self.on(self.blueprint).fg(self.text_strong)
    }

    pub const fn ansi16(self) -> bool {
        self.ansi16
    }

    pub fn canvas_style(self) -> Style {
        self.on(self.canvas).fg(self.text)
    }

    pub fn panel_style(self) -> Style {
        self.on(self.panel).fg(self.text)
    }

    pub fn raised_style(self) -> Style {
        self.on(self.raised).fg(self.text)
    }

    pub fn selection_style(self) -> Style {
        self.on(self.selection)
            .fg(self.text_strong)
            .add_modifier(Modifier::BOLD)
    }

    pub fn key_style(self) -> Style {
        if self.no_color {
            Style::default()
                .fg(self.amber)
                .add_modifier(Modifier::BOLD | Modifier::REVERSED)
        } else if self.ansi16 {
            Style::default()
                .fg(Color::LightYellow)
                .bg(Color::Black)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(self.amber)
                .bg(self.raised)
                .add_modifier(Modifier::BOLD)
        }
    }

    pub fn on(self, background: Color) -> Style {
        if self.no_color {
            Style::default()
        } else {
            Style::default().bg(background)
        }
    }

    pub const fn no_color(self) -> bool {
        self.no_color
    }
}

const fn rgb((red, green, blue): (u8, u8, u8)) -> Color {
    Color::Rgb(red, green, blue)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substrate_dark_uses_distinct_semantic_layers() {
        let theme = Theme::new(false);
        assert_eq!(theme.canvas, Color::Rgb(0, 31, 39));
        assert_ne!(theme.canvas, theme.panel);
        assert_ne!(theme.panel, theme.raised);
        assert_ne!(theme.cyan, theme.amber);
        assert_ne!(theme.warning, theme.danger);
    }

    #[test]
    fn monochrome_theme_does_not_force_terminal_colors() {
        let theme = Theme::new(true);
        assert!(theme.no_color());
        assert!(theme
            .canvas_style()
            .fg
            .is_none_or(|color| color == Color::Reset));
        assert!(theme
            .canvas_style()
            .bg
            .is_none_or(|color| color == Color::Reset));
        assert!(theme.key_style().add_modifier.contains(Modifier::REVERSED));
    }

    #[test]
    fn essential_text_and_state_colors_remain_legible_on_surfaces() {
        for foreground in [
            TEXT_RGB,
            CYAN_RGB,
            CYAN_BRIGHT_RGB,
            AMBER_RGB,
            GOLD_RGB,
            COBALT_BRIGHT_RGB,
            MAGENTA_RGB,
            ORANGE_RGB,
            RED_RGB,
            GREEN_RGB,
        ] {
            assert!(contrast_ratio(foreground, PANEL_RGB) >= 4.5);
        }
        for foreground in [
            TEXT_RGB,
            CYAN_BRIGHT_RGB,
            AMBER_RGB,
            GOLD_RGB,
            COBALT_BRIGHT_RGB,
            MAGENTA_RGB,
            ORANGE_RGB,
            RED_RGB,
            GREEN_RGB,
        ] {
            assert!(contrast_ratio(foreground, RAISED_RGB) >= 4.5);
        }
        for foreground in [
            TEXT_STRONG_RGB,
            CYAN_BRIGHT_RGB,
            GOLD_RGB,
            COBALT_BRIGHT_RGB,
        ] {
            assert!(contrast_ratio(foreground, BLUEPRINT_RGB) >= 4.5);
        }
    }

    #[test]
    fn ansi16_theme_uses_standard_indexed_ega_colors_without_rgb() {
        let theme = Theme::for_color_mode(false, true);
        assert!(theme.ansi16());
        assert!(!theme.no_color());
        assert_eq!(theme.blueprint, Color::Blue);
        assert_eq!(theme.cyan_bright, Color::LightCyan);
        assert_eq!(theme.gold, Color::LightYellow);
        assert_eq!(theme.magenta, Color::LightMagenta);
    }

    fn contrast_ratio(left: (u8, u8, u8), right: (u8, u8, u8)) -> f64 {
        let left = relative_luminance(left);
        let right = relative_luminance(right);
        (left.max(right) + 0.05) / (left.min(right) + 0.05)
    }

    fn relative_luminance((red, green, blue): (u8, u8, u8)) -> f64 {
        let channel = |value: u8| {
            let value = f64::from(value) / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        channel(red) * 0.2126 + channel(green) * 0.7152 + channel(blue) * 0.0722
    }
}
