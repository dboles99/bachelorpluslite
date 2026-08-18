//! Theme data for BachelorPad+.
//!
//! ADR-0009 requires four built-in themes that restyle every surface, defined
//! as data so that user themes use the same mechanism the built-ins do. That
//! is why this crate holds colours and no drawing code, and why the UI layer
//! reads its palette from here rather than hardcoding one: a theme the user
//! writes has to be indistinguishable from a theme we ship.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// A colour, 8 bits per channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Parse `#rrggbb` or `rrggbb`.
    ///
    /// Returns `None` rather than a default colour: a typo in a user's theme
    /// file should be reportable, not silently rendered as black.
    pub fn from_hex(hex: &str) -> Option<Self> {
        let s = hex.strip_prefix('#').unwrap_or(hex);
        if s.len() != 6 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        Some(Self {
            r: u8::from_str_radix(&s[0..2], 16).ok()?,
            g: u8::from_str_radix(&s[2..4], 16).ok()?,
            b: u8::from_str_radix(&s[4..6], 16).ok()?,
        })
    }

    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

/// The surfaces a theme colours.
///
/// Deliberately small and role-named rather than widget-named. `panel` means
/// "the surface content sits on", so a new widget added later already has a
/// correct colour without extending this struct.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Palette {
    /// Window chrome behind everything.
    pub shell: Rgb,
    /// Content surfaces: editor body, menu bar, status bar.
    pub panel: Rgb,
    /// Separators and borders.
    pub edge: Rgb,
    /// Primary text.
    pub ink: Rgb,
    /// Secondary text: gutter, status cells, inactive tabs.
    pub ink_dim: Rgb,
    /// Emphasis: save state, theme indicator, selection.
    pub accent: Rgb,
}

/// Which built-in theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemeId {
    Light,
    Dark,
    Organic,
    /// The default. Changing this changes what every new installation opens
    /// to, so it is pinned by a test rather than left to whichever variant
    /// happens to be listed first.
    #[default]
    Green,
}

impl ThemeId {
    pub const ALL: [Self; 4] = [Self::Light, Self::Dark, Self::Organic, Self::Green];

    /// The next theme in the cycle, wrapping.
    pub const fn next(self) -> Self {
        match self {
            Self::Light => Self::Dark,
            Self::Dark => Self::Organic,
            Self::Organic => Self::Green,
            Self::Green => Self::Light,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Light => "Light",
            Self::Dark => "Dark",
            Self::Organic => "Organic",
            Self::Green => "Green",
        }
    }

    /// Look up by display name, case-insensitively.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|t| t.name().eq_ignore_ascii_case(name))
    }

    /// The theme that matches the desktop's light/dark preference.
    ///
    /// Resolves to one of the four rather than becoming a fifth. ADR-0009
    /// makes themes data, and a "System" palette would be one the user could
    /// never open, edit or override -- so "follow the system" is a *choice
    /// about which theme to use*, not a theme.
    ///
    /// `None` means the platform would not say, which is not an error: a
    /// desktop with no such preference is normal, and the caller keeps what
    /// it had rather than guessing.
    #[must_use]
    pub const fn for_system(dark: Option<bool>) -> Option<Self> {
        match dark {
            // Dark maps to Dark rather than to the Green default: someone who
            // asked their desktop for dark has said what they want, and Green
            // is a dark-ish theme with a colour cast they did not ask for.
            Some(true) => Some(Self::Dark),
            Some(false) => Some(Self::Light),
            None => None,
        }
    }

    pub const fn palette(self) -> Palette {
        match self {
            Self::Light => Palette {
                shell: Rgb::new(0xe8, 0xe6, 0xe1),
                panel: Rgb::new(0xf7, 0xf6, 0xf3),
                edge: Rgb::new(0xc2, 0xbd, 0xb4),
                ink: Rgb::new(0x1b, 0x1b, 0x1b),
                ink_dim: Rgb::new(0x5f, 0x5f, 0x5f),
                accent: Rgb::new(0xb8, 0x54, 0x1f),
            },
            Self::Dark => Palette {
                shell: Rgb::new(0x23, 0x26, 0x2b),
                panel: Rgb::new(0x1a, 0x1d, 0x21),
                edge: Rgb::new(0x3a, 0x3f, 0x47),
                ink: Rgb::new(0xe6, 0xe6, 0xe6),
                ink_dim: Rgb::new(0x9a, 0xa0, 0xa8),
                accent: Rgb::new(0x4c, 0x9b, 0xe8),
            },
            Self::Organic => Palette {
                shell: Rgb::new(0x2f, 0x26, 0x1c),
                panel: Rgb::new(0x3a, 0x2f, 0x22),
                edge: Rgb::new(0x5a, 0x4a, 0x34),
                ink: Rgb::new(0xf0, 0xe4, 0xd2),
                ink_dim: Rgb::new(0xb9, 0xa1, 0x84),
                accent: Rgb::new(0xd9, 0x88, 0x29),
            },
            Self::Green => Palette {
                shell: Rgb::new(0x08, 0x16, 0x0c),
                panel: Rgb::new(0x0d, 0x23, 0x12),
                edge: Rgb::new(0x1c, 0x4a, 0x28),
                ink: Rgb::new(0xb8, 0xf5, 0xc4),
                ink_dim: Rgb::new(0x6f, 0xbf, 0x82),
                accent: Rgb::new(0x3d, 0xdc, 0x6a),
            },
        }
    }

    /// The whole theme as data, ready to serialise as a starting point for a
    /// user theme.
    pub fn theme(self) -> Theme {
        Theme {
            name: self.name().to_owned(),
            palette: self.palette(),
        }
    }
}

/// A theme, built-in or user-supplied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Theme {
    pub name: String,
    pub palette: Palette,
}

/// Crate identity used by workspace smoke tests and diagnostics.
pub const CRATE_NAME: &str = "bp-theme";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_with_and_without_hash() {
        assert_eq!(Rgb::from_hex("#e8e6e1"), Some(Rgb::new(0xe8, 0xe6, 0xe1)));
        assert_eq!(Rgb::from_hex("e8e6e1"), Some(Rgb::new(0xe8, 0xe6, 0xe1)));
        assert_eq!(Rgb::from_hex("E8E6E1"), Some(Rgb::new(0xe8, 0xe6, 0xe1)));
    }

    #[test]
    fn rejects_malformed_hex_rather_than_defaulting() {
        // A typo in a user's theme file must be reportable, not silently black.
        assert_eq!(Rgb::from_hex(""), None);
        assert_eq!(Rgb::from_hex("#fff"), None);
        assert_eq!(Rgb::from_hex("#gggggg"), None);
        assert_eq!(Rgb::from_hex("#e8e6e1e8"), None);
    }

    #[test]
    fn hex_round_trips() {
        for id in ThemeId::ALL {
            let p = id.palette();
            for c in [p.shell, p.panel, p.edge, p.ink, p.ink_dim, p.accent] {
                assert_eq!(Rgb::from_hex(&c.to_hex()), Some(c));
            }
        }
    }

    #[test]
    fn cycling_visits_every_theme_and_returns() {
        let mut seen = Vec::new();
        let mut t = ThemeId::Light;
        for _ in 0..ThemeId::ALL.len() {
            seen.push(t);
            t = t.next();
        }
        assert_eq!(t, ThemeId::Light, "cycle must wrap");
        for id in ThemeId::ALL {
            assert!(seen.contains(&id), "{id:?} unreachable by cycling");
        }
    }

    #[test]
    fn themes_are_visually_distinct() {
        // Two themes sharing a palette would be a copy-paste slip that no
        // other test would catch.
        let palettes: Vec<_> = ThemeId::ALL.into_iter().map(ThemeId::palette).collect();
        for (i, a) in palettes.iter().enumerate() {
            for b in palettes.iter().skip(i + 1) {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn text_contrasts_with_its_surface() {
        // Not a WCAG check -- a crude guard that no theme ships ink that
        // vanishes into its own panel.
        for id in ThemeId::ALL {
            let p = id.palette();
            let lum =
                |c: Rgb| 0.299 * f32::from(c.r) + 0.587 * f32::from(c.g) + 0.114 * f32::from(c.b);
            let delta = (lum(p.ink) - lum(p.panel)).abs();
            assert!(delta > 80.0, "{} ink/panel delta only {delta}", id.name());
        }
    }

    #[test]
    fn the_default_theme_is_green() {
        // What a new installation opens to. Deliberate, not incidental.
        assert_eq!(ThemeId::default(), ThemeId::Green);
    }

    #[test]
    fn looks_up_by_name() {
        assert_eq!(ThemeId::from_name("organic"), Some(ThemeId::Organic));
        assert_eq!(ThemeId::from_name("GREEN"), Some(ThemeId::Green));
        assert_eq!(ThemeId::from_name("Puce"), None);
    }

    #[test]
    fn following_the_system_resolves_to_a_theme_that_already_exists() {
        // ADR-0009 makes themes data. "System" resolving to a fifth palette
        // would be one nobody could open, edit or override, so this asserts
        // it always lands on one of the four.
        for dark in [Some(true), Some(false)] {
            let resolved = ThemeId::for_system(dark).expect("a stated preference resolves");
            assert!(
                ThemeId::ALL.contains(&resolved),
                "{resolved:?} is not one of the built-in themes"
            );
        }
        assert_eq!(ThemeId::for_system(Some(true)), Some(ThemeId::Dark));
        assert_eq!(ThemeId::for_system(Some(false)), Some(ThemeId::Light));
    }

    #[test]
    fn a_desktop_that_will_not_say_gets_no_answer_rather_than_a_guess() {
        // Not an error and not a default: the caller keeps the theme it had.
        // Returning `Green` here would silently override a user's choice on
        // every platform that does not report a preference.
        assert_eq!(ThemeId::for_system(None), None);
    }

    #[test]
    fn a_theme_round_trips_through_json() {
        // User themes are the same shape as built-ins (ADR-0009).
        let theme = ThemeId::Organic.theme();
        let json = serde_json::to_string(&theme).unwrap();
        assert_eq!(serde_json::from_str::<Theme>(&json).unwrap(), theme);
    }
}
