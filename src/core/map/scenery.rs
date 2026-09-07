//! Sourced celestial animation metadata shared by asset loading and map presentation.

/// One compact looping animation, independent of authoritative game state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CelestialKind {
    BlackHole,
    NeutronStar,
    Magnetar,
}

impl CelestialKind {
    pub(crate) const ALL: [Self; 3] = [Self::BlackHole, Self::NeutronStar, Self::Magnetar];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::BlackHole => "black hole",
            Self::NeutronStar => "neutron star",
            Self::Magnetar => "magnetar",
        }
    }

    pub(crate) fn frame_count(self) -> usize {
        match self {
            Self::BlackHole => 66,
            Self::NeutronStar | Self::Magnetar => 48,
        }
    }

    pub(crate) fn frame_seconds(self) -> f32 {
        match self {
            Self::BlackHole => 0.08,
            Self::NeutronStar => 0.375,
            Self::Magnetar => 0.125,
        }
    }

    /// Keeps every selected landmark large enough to recognize at the widest map zoom.
    pub(crate) fn size_scale(self) -> f32 {
        let _ = self;
        1.0
    }

    pub(crate) fn opacity(self) -> f32 {
        match self {
            Self::BlackHole => 0.7,
            Self::NeutronStar => 0.85,
            Self::Magnetar => 0.75,
        }
    }
}
