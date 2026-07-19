//! `boast` — a reproducible research-impact aggregator.
//!
//! Point it at a [`Project`](model::Project) (a paper, and later a repo and
//! packages) and it fetches reach [`Metric`](model::Metric)s from pluggable
//! [`Provider`](provider::Provider)s through a single
//! [`Transport`](transport::Transport) seam, records them in a durable
//! [`Snapshot`](model::Snapshot), and renders a [report](report).

pub mod cli;
pub mod model;
pub mod orchestrator;
pub mod provider;
pub mod providers;
pub mod report;
pub mod transport;

pub use model::{
    Category, FetchResult, Identity, Metric, MetricValue, Outcome, PaperId, Project, Snapshot,
    Window,
};
