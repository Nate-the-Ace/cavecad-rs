//! Which instrument is this? One Connect button serves every supported instrument, so the one chosen has to
//! be told from the others. The Cavway X1 and the SAP5 offer the very same Bluetooth service (Nordic UART), so
//! the service alone cannot do it: TopoDroid goes by the name the instrument advertises, and so does this.

use crate::{bric, caveble, cavway};
use serde::Serialize;

/// The kinds of instrument.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Cavway X1.
    X1,
    /// Shetland Attack Pony 5 (DistoX packets).
    Sap5,
    /// SAP6 (CaveBLE).
    Sap6,
    /// BRIC4 and BRIC5.
    Bric,
    /// Known, not built.
    Unsupported,
}

/// What an instrument advertises its name starts with, and what to call it.
pub struct InstrumentName {
    /// Advertised name prefix.
    pub prefix: &'static str,
    /// Its kind.
    pub kind: Kind,
    /// What to call it.
    pub label: &'static str,
}

/// The names the supported instruments advertise (from TopoDroid's device list).
pub const NAMES: &[InstrumentName] = &[
    InstrumentName {
        prefix: "CavwayX1-",
        kind: Kind::X1,
        label: "Cavway X1",
    },
    InstrumentName {
        prefix: "SAP6_",
        kind: Kind::Sap6,
        label: "SAP6",
    },
    InstrumentName {
        prefix: "SAP_",
        kind: Kind::Sap6,
        label: "SAP6",
    },
    InstrumentName {
        prefix: "SAP5_",
        kind: Kind::Sap5,
        label: "SAP5",
    },
    InstrumentName {
        prefix: "Shetland_",
        kind: Kind::Sap5,
        label: "SAP5",
    },
    InstrumentName {
        prefix: "BRIC4_",
        kind: Kind::Bric,
        label: "BRIC4",
    },
    InstrumentName {
        prefix: "BRIC5-",
        kind: Kind::Bric,
        label: "BRIC5",
    },
    InstrumentName {
        prefix: "BRIC5_",
        kind: Kind::Bric,
        label: "BRIC5",
    },
    InstrumentName {
        prefix: "BRIC-",
        kind: Kind::Bric,
        label: "BRIC",
    },
    // Known, and in TopoDroid, but not built here yet.
    InstrumentName {
        prefix: "DistoXBLE-",
        kind: Kind::Unsupported,
        label: "DistoX BLE",
    },
];

/// How an instrument was told apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum How {
    /// By the advertised name.
    Name,
    /// By what it offers (a name this does not know).
    Service,
}

/// A recognised instrument.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Identified {
    /// Its kind.
    pub kind: Kind,
    /// What to call it.
    pub label: &'static str,
    /// How it was told.
    pub how: How,
}

/// Names the instrument from its advertised name and, failing that, the Bluetooth services it offers. A
/// service that only one instrument has settles it (the CaveBLE service is the SAP6's, the 0x58D0 service
/// the BRIC's); the Nordic UART service alone does not (X1, SAP5 and others share it), so `None` is returned
/// and the caller reads the packets to tell.
pub fn identify(name: &str, services: &[String]) -> Option<Identified> {
    let n = name.trim();
    if let Some(x) = NAMES.iter().find(|x| n.starts_with(x.prefix)) {
        return Some(Identified {
            kind: x.kind,
            label: x.label,
            how: How::Name,
        });
    }
    let has = |uuid: &str| services.iter().any(|s| s.eq_ignore_ascii_case(uuid));
    if has(caveble::SERVICE) {
        return Some(Identified {
            kind: Kind::Sap6,
            label: "SAP6",
            how: How::Service,
        });
    }
    if has(&bric::measure_service()) {
        return Some(Identified {
            kind: Kind::Bric,
            label: "BRIC",
            how: How::Service,
        });
    }
    None
}

/// Whether the service list says Nordic UART is there (the X1 and SAP5 both), for a name this does not know.
pub fn has_nordic_uart(services: &[String]) -> bool {
    services
        .iter()
        .any(|s| s.eq_ignore_ascii_case(cavway::SERVICE))
}
