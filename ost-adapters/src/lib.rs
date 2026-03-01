//! Game-specific telemetry adapters for OpenSimTelemetry

pub mod demo;
pub mod ibt_parser;
pub mod iracing;

pub use demo::DemoAdapter;
pub use iracing::IRacingAdapter;

use ost_core::model::TrackSurface;

/// Map iRacing `irsdk_TrkSurf` enum values to our normalised [`TrackSurface`].
///
/// Values derived from the iRacing SDK header `irsdk_TrkSurf` (C enum, auto-incrementing from 0).
pub fn iracing_track_surface(idx: i32) -> TrackSurface {
    match idx {
        -1 => TrackSurface::NotInWorld,
        0 => TrackSurface::Undefined,
        1..=4 => TrackSurface::Asphalt,
        5 | 6 => TrackSurface::Concrete,
        7 | 8 => TrackSurface::RacingDirt,
        9 | 10 => TrackSurface::Paint,
        11..=14 => TrackSurface::Rumble,
        15..=18 => TrackSurface::Grass,
        19..=22 => TrackSurface::Dirt,
        23 => TrackSurface::Sand,
        24 | 25 => TrackSurface::Gravel,
        26 => TrackSurface::Grasscrete,
        27 => TrackSurface::Astroturf,
        _ => TrackSurface::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_iracing_track_surface_mapping() {
        assert_eq!(iracing_track_surface(-1), TrackSurface::NotInWorld);
        assert_eq!(iracing_track_surface(0), TrackSurface::Undefined);
        // Asphalt variants 1-4
        for i in 1..=4 {
            assert_eq!(iracing_track_surface(i), TrackSurface::Asphalt);
        }
        // Concrete 5-6
        assert_eq!(iracing_track_surface(5), TrackSurface::Concrete);
        assert_eq!(iracing_track_surface(6), TrackSurface::Concrete);
        // RacingDirt 7-8
        assert_eq!(iracing_track_surface(7), TrackSurface::RacingDirt);
        assert_eq!(iracing_track_surface(8), TrackSurface::RacingDirt);
        // Paint 9-10
        assert_eq!(iracing_track_surface(9), TrackSurface::Paint);
        assert_eq!(iracing_track_surface(10), TrackSurface::Paint);
        // Rumble 11-14
        for i in 11..=14 {
            assert_eq!(iracing_track_surface(i), TrackSurface::Rumble);
        }
        // Grass 15-18
        for i in 15..=18 {
            assert_eq!(iracing_track_surface(i), TrackSurface::Grass);
        }
        // Dirt 19-22
        for i in 19..=22 {
            assert_eq!(iracing_track_surface(i), TrackSurface::Dirt);
        }
        // Sand 23
        assert_eq!(iracing_track_surface(23), TrackSurface::Sand);
        // Gravel 24-25
        assert_eq!(iracing_track_surface(24), TrackSurface::Gravel);
        assert_eq!(iracing_track_surface(25), TrackSurface::Gravel);
        // Grasscrete 26
        assert_eq!(iracing_track_surface(26), TrackSurface::Grasscrete);
        // Astroturf 27
        assert_eq!(iracing_track_surface(27), TrackSurface::Astroturf);
        // Unknown for anything else
        assert_eq!(iracing_track_surface(28), TrackSurface::Unknown);
        assert_eq!(iracing_track_surface(100), TrackSurface::Unknown);
    }
}
