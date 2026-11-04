//! The OctoSense app hub (ADR 0003).
//!
//! Three sides of one boundary:
//!
//! - [`gate`] decides whether a bundle is admitted. It runs identically on a
//!   developer's machine before submitting and in the hub's job after, so the
//!   report a publisher sees is the report the hub acts on.
//! - [`index`] is what gets published: an entry per app version, and the
//!   signed catalog a device reads.
//! - [`client`] is the device's side: verify, list, search, install, remove,
//!   and refuse to run what has been withdrawn.
//!
//! What an app may DO once installed is not here — that is ADR 0002 and the
//! `octosense-app-policy` crate. The hub decides what is offered; the policy
//! and the runtime decide what is allowed.
pub mod admission;
pub mod advisories;
pub mod client;
pub mod components;
pub mod functions;
pub mod gate;
pub mod github_catalog;
pub mod github_publisher;
pub mod index;
pub(crate) mod launch;
pub mod pack;
pub mod publishers;
pub mod publisher_tools;
pub mod remote;
pub mod scan;
pub mod signing;

pub use client::{
    adopt_legacy_install, adopt_legacy_installs, days_between, install_root, installed_bundle_dir, AppAvailability, Availability, Listing,
    PreparedLaunch, Store, CATALOG_FRESHNESS_DAYS, INSTALLS_DIR,
};
pub use pack::{pack_dir, unpack, Pack};
pub use remote::{today, Remote};
pub use scan::{packet, scan, Packet, Route, Verdict};
pub use gate::{check_bundle, check_component, component_entry_for, entry_for, ComponentReport, Finding, GateReport, Severity};
pub use index::{Catalog, Entry, Source, Status, WorkingKey, CATALOG_SCHEMA};
pub use components::{ComponentEntry, ComponentListing, ComponentManifest, ComponentRelease, Resolved};
pub use publishers::{verify_continuity, CatalogPublishers, PublisherBinding, PublisherRegistry};
pub use signing::{sign_manifest, verify_catalog, HubKey, PublisherKeys};

/// The hub anchor OctoSense stores trust (ADR 0003 §4): the hub's working
/// key is certified by it, so rotating that key needs no store release.
/// `hub check` and `hub publish` authenticate an existing catalog against it
/// unless `--anchor` names another (a development hub).
pub const DEFAULT_ANCHOR: &str = "6000284a069ba7cada2925094074e8e0baae07e25d1b7fc31f396c993f363e11";
