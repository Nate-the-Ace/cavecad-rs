//! The geometry behind the sketchpad: the survey turned into the three flat views a cave sketch is made on
//! (plan, profile, cross section), and strokes that stay attached to the survey.
//!
//! The views are the ordinary ones: PLAN looks straight down (x east, y north); PROFILE is the extended
//! elevation, the main chain unrolled onto one horizontal axis (distance along the passage, elevation);
//! SECTION looks along the passage at one station (across to the right, up). Walls come from the passage
//! dimensions (L R U D). A stroke is stored as offsets from the nearest station in its view, never as screen
//! or world coordinates, so it moves with the survey if a station shifts.
//!
//! Pure: a [`Source`] goes in (or a notebook [`cave_notebook::Page`] through [`source`]), plain views come out.

mod layout;
mod source;
mod stroke;
mod views;

pub use layout::{Layout, Leg, Splay, Station, layout, shot_vector};
pub use source::{Dims, DimsByStation, Source, SurveyShot, source};
pub use stroke::{Anchored, anchor_stroke, place_stroke};
pub use views::{Mark, PlanView, ProfileView, SectionView, plan_view, profile_view, section_view};

/// A point in a view's own metres (y up).
pub type Pt = [f64; 2];
