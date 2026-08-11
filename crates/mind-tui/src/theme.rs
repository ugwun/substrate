use ratatui::style::{Color, Modifier, Style};

pub const CANVAS_RGB: (u8, u8, u8) = (0, 31, 39);
pub const PANEL_RGB: (u8, u8, u8) = (5, 45, 54);
pub const RAISED_RGB: (u8, u8, u8) = (7, 54, 66);
pub const CYAN_RGB: (u8, u8, u8) = (42, 161, 152);
pub const CYAN_BRIGHT_RGB: (u8, u8, u8) = (94, 196, 189);
pub const AMBER_RGB: (u8, u8, u8) = (203, 155, 22);

const TEXT_RGB: (u8, u8, u8) = (147, 161, 161);
const TEXT_STRONG_RGB: (u8, u8, u8) = (238, 232, 213);
const MUTED_RGB: (u8, u8, u8) = (101, 123, 131);
const FAINT_RGB: (u8, u8, u8) = (69, 94, 101);
const BORDER_RGB: (u8, u8, u8) = (20, 77, 87);
const SELECTION_RGB: (u8, u8, u8) = (10, 68, 77);
const BLUE_RGB: (u8, u8, u8) = (38, 139, 210);
const ORANGE_RGB: (u8, u8, u8) = (238, 125, 70);
const RED_RGB: (u8, u8, u8) = (245, 110, 105);
const GREEN_RGB: (u8, u8, u8) = (154, 174, 30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    no_color: bool,
    pub canvas: Color,
    pub panel: Color,
    pub raised: Color,
    pub text: Color,
    pub text_strong: Color,
    pub muted: Color,
    pub faint: Color,
    pub border: Color,
    pub selection: Color,
    pub cyan: Color,
    pub cyan_bright: Color,
    pub amber: Color,
    pub blue: Color,
    pub warning: Color,
    pub danger: Color,
    pub success: Color,
}

impl Theme {
    pub fn new(no_color: bool) -> Self {
        if no_color {
            return Self {
                no_color,
                canvas: Color::Reset,
                panel: Color::Reset,
                raised: Color::Reset,
                text: Color::Reset,
                text_strong: Color::Reset,
                muted: Color::Reset,
                faint: Color::Reset,
                border: Color::Reset,
                selection: Color::Reset,
                cyan: Color::Reset,
                cyan_bright: Color::Reset,
                amber: Color::Reset,
                blue: Color::Reset,
                warning: Color::Reset,
                danger: Color::Reset,
                success: Color::Reset,
            };
        }

        Self {
            no_color,
            canvas: rgb(CANVAS_RGB),
            panel: rgb(PANEL_RGB),
            raised: rgb(RAISED_RGB),
            text: rgb(TEXT_RGB),
            text_strong: rgb(TEXT_STRONG_RGB),
            muted: rgb(MUTED_RGB),
            faint: rgb(FAINT_RGB),
            border: rgb(BORDER_RGB),
            selection: rgb(SELECTION_RGB),
            cyan: rgb(CYAN_RGB),
            cyan_bright: rgb(CYAN_BRIGHT_RGB),
            amber: rgb(AMBER_RGB),
            blue: rgb(BLUE_RGB),
            warning: rgb(ORANGE_RGB),
            danger: rgb(RED_RGB),
            success: rgb(GREEN_RGB),
        }
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
        let style = Style::default().fg(self.amber).add_modifier(Modifier::BOLD);
        if self.no_color {
            style.add_modifier(Modifier::REVERSED)
        } else {
            style.bg(self.raised)
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
            ORANGE_RGB,
            RED_RGB,
            GREEN_RGB,
        ] {
            assert!(contrast_ratio(foreground, RAISED_RGB) >= 4.5);
        }
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
