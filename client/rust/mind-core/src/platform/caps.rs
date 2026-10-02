// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Platform capability descriptor (plan 22 M0 §3.3).
//!
//! Ported from `core/src/mindustry/Vars.java` (`mobile`, `steam`, `headless`)
//! and `mindustry/core/Platform.java` (platform identity). Boot data consumed by
//! plans 14/15/16/17/18 to pick mobile layouts, FX budgets and renderer
//! defaults; never consulted by simulation systems (plan 05 invariant 3).

/// Which host family the process runs on (`Platform.java` backend identity).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformKind {
    /// Desktop (Windows/Linux/macOS) client.
    Desktop,
    /// Android client.
    Android,
    /// iOS client (authored, not shipped this phase — NUD-52=B).
    Ios,
    /// Headless server / harness (`Vars.headless`).
    Headless,
}

/// Coarse performance class used for graphics/particle/FX defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PerformanceTier {
    /// Low-end mobile / fallback.
    Low,
    /// Mid-range.
    Medium,
    /// Desktop-class.
    High,
}

impl PerformanceTier {
    /// Selects a tier from a logical processor count.
    ///
    /// Android defaults to [`PerformanceTier::Low`] regardless of core count
    /// (plan 22 §3.8); desktop is `High` at 8+ cores and `Medium` otherwise.
    pub fn from_processor_count(kind: PlatformKind, cores: u32) -> Self {
        if kind == PlatformKind::Android || kind == PlatformKind::Ios {
            return PerformanceTier::Low;
        }
        if cores >= 8 {
            PerformanceTier::High
        } else {
            PerformanceTier::Medium
        }
    }
}

/// Platform capability snapshot (`Vars.mobile`/`steam`/`headless` + tier).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformCaps {
    /// Host family.
    pub kind: PlatformKind,
    /// `Vars.headless`.
    pub headless: bool,
    /// `Vars.mobile` (real mobile target; distinct from `test_mobile`).
    pub mobile: bool,
    /// `Vars.testMobile` (`-testMobile` / `--mobile-preview`).
    pub test_mobile: bool,
    /// Active locale (`Core.settings "locale"` or OS default).
    pub locale: String,
    /// Default locale for the build.
    pub locale_default: String,
    /// Graphics/particle tier.
    pub tier: PerformanceTier,
    /// `Vars.steam` (false until OD4 lands).
    pub steam: bool,
}

impl Default for PlatformCaps {
    fn default() -> Self {
        Self::headless()
    }
}

impl PlatformCaps {
    /// A desktop-client snapshot.
    pub fn desktop(locale: impl Into<String>) -> Self {
        let locale = locale.into();
        Self {
            kind: PlatformKind::Desktop,
            headless: false,
            mobile: false,
            test_mobile: false,
            locale_default: locale.clone(),
            locale,
            tier: PerformanceTier::High,
            steam: false,
        }
    }

    /// A mobile-client snapshot (`Vars.mobile`).
    pub fn mobile(kind: PlatformKind, locale: impl Into<String>) -> Self {
        let locale = locale.into();
        Self {
            kind,
            headless: false,
            mobile: true,
            test_mobile: false,
            locale_default: locale.clone(),
            locale,
            tier: PerformanceTier::Low,
            steam: false,
        }
    }

    /// A headless-server snapshot.
    pub fn headless() -> Self {
        Self {
            kind: PlatformKind::Headless,
            headless: true,
            mobile: false,
            test_mobile: false,
            locale: "en".to_owned(),
            locale_default: "en".to_owned(),
            tier: PerformanceTier::High,
            steam: false,
        }
    }

    /// Whether the UI should take the mobile layout branch
    /// (`mobile || test_mobile`, plan 14 §3.11).
    pub fn is_mobile_ui(&self) -> bool {
        self.mobile || self.test_mobile
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mobile_implies_not_headless() {
        let caps = PlatformCaps::mobile(PlatformKind::Android, "en");
        assert!(caps.mobile);
        assert!(caps.is_mobile_ui());
        assert!(!caps.headless);
        assert_eq!(caps.kind, PlatformKind::Android);
        assert_eq!(caps.tier, PerformanceTier::Low);
    }

    #[test]
    fn headless_caps() {
        let caps = PlatformCaps::headless();
        assert!(caps.headless);
        assert!(!caps.mobile);
        assert!(!caps.is_mobile_ui());
        assert_eq!(caps.kind, PlatformKind::Headless);
    }

    #[test]
    fn test_mobile_enables_mobile_ui() {
        let mut caps = PlatformCaps::desktop("en");
        caps.test_mobile = true;
        assert!(caps.is_mobile_ui());
        assert!(!caps.mobile);
    }

    #[test]
    fn tier_from_processor_count() {
        assert_eq!(
            PerformanceTier::from_processor_count(PlatformKind::Desktop, 16),
            PerformanceTier::High
        );
        assert_eq!(
            PerformanceTier::from_processor_count(PlatformKind::Desktop, 4),
            PerformanceTier::Medium
        );
        assert_eq!(
            PerformanceTier::from_processor_count(PlatformKind::Android, 16),
            PerformanceTier::Low
        );
        assert_eq!(
            PerformanceTier::from_processor_count(PlatformKind::Headless, 4),
            PerformanceTier::Medium
        );
    }
}
