//! Builds the index and rolls the serving Deployment out, from the chart's ConfigMap: the index
//! id the values name, the build Job and the serving Deployment, both rendered by Helm.
//!
//! Every round, stateless: if the store has no sealed `indexes/<id>/_COMPLETE`, make sure one
//! build Job for that id exists (a failed one is replaced after a delay) and leave the serving
//! Deployment as it is; once the index is sealed, delete the build Jobs and server-side apply the
//! Deployment, whose rolling update starts the new pods with an index they don't have to wait
//! for. No serving pod exists before the first index does. The bucket must already exist: the
//! leader exits when it can't list it.
//!
//! Replicas elect one leader through a Lease (lease.rs).
//!
//! Under Argo CD (the leader's own Deployment carries a tracking id), what the leader creates gets
//! a tracking id of its own instead of an owner reference, so the app shows it as a top-level
//! resource; it is marked to not be pruned, since the app's source doesn't render it.
mod lease;
#[path = "../build_index/mem.rs"]
#[allow(dead_code)]
mod mem;
#[path = "../build_index/progress.rs"]
#[allow(dead_code)]
mod progress;
#[path = "../build_index/store.rs"]
#[allow(dead_code)]
mod store;

use std::str::FromStr;
use std::time::Duration;

use anyhow::{Context, Result};
use k8s_openapi::api::apps::v1::Deployment;
use k8s_openapi::api::batch::v1::Job;
use k8s_openapi::api::core::v1::ConfigMap;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{ObjectMeta, OwnerReference};
use k8s_openapi::jiff::Timestamp;
use kube::api::{Api, DeleteParams, ListParams, Patch, PatchParams, PostParams};
use store::Store;

const MANAGER: &str = "opengeocoding-leader";
const LABEL: &str = "opengeocoding.io/leader";
const TRACKING: &str = "argocd.argoproj.io/tracking-id";

/// Who removes what the leader creates: Argo CD's app `app`, or the leader's Deployment.
enum Owner {
    ArgoCd(String),
    Leader(OwnerReference),
}

impl Owner {
    fn claim(&self, meta: &mut ObjectMeta, group_kind: &str, namespace: &str) {
        meta.namespace = Some(namespace.into());
        let app = match self {
            Owner::ArgoCd(app) => app,
            Owner::Leader(owner) => {
                meta.owner_references = Some(vec![owner.clone()]);
                return;
            }
        };
        let name = meta.name.clone().unwrap_or_default();
        let annotations = meta.annotations.get_or_insert_default();
        annotations.insert(
            TRACKING.into(),
            format!("{app}:{group_kind}:{namespace}/{name}"),
        );
        annotations.insert(
            "argocd.argoproj.io/compare-options".into(),
            "IgnoreExtraneous".into(),
        );
        annotations.insert(
            "argocd.argoproj.io/sync-options".into(),
            "Prune=false".into(),
        );
    }
}

fn env<T: FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn required(key: &str) -> Result<String> {
    std::env::var(key).with_context(|| format!("{key} is required"))
}

#[derive(Debug, PartialEq)]
enum Build {
    Create,
    Running,
    /// Finished while the index isn't published, long enough ago to try again.
    Replace(String),
    Waiting(String),
}

/// What to do about the build Job for the wanted index. A Job never retries its pod
/// (`backoffLimit: 0`): this is the one retry policy.
fn decide(job: Option<&Job>, now: Timestamp, retry: Duration) -> Build {
    let Some(job) = job else {
        return Build::Create;
    };
    let finished = (job.status.iter())
        .flat_map(|s| s.conditions.iter().flatten())
        .find(|c| matches!(c.type_.as_str(), "Complete" | "Failed") && c.status == "True");
    let Some(c) = finished else {
        return Build::Running;
    };
    let why = match c.type_.as_str() {
        "Failed" => format!(
            "failed: {}",
            c.message.as_deref().or(c.reason.as_deref()).unwrap_or("")
        ),
        _ => "finished without publishing the index".to_string(),
    };
    let age = c.last_transition_time.as_ref().map_or(i64::MAX, |t| {
        now.as_second().saturating_sub(t.0.as_second())
    });
    if age >= retry.as_secs() as i64 {
        Build::Replace(why)
    } else {
        Build::Waiting(format!(
            "{why}; trying again in {}s",
            retry.as_secs() as i64 - age
        ))
    }
}

struct Leader {
    store: Store,
    configs: Api<ConfigMap>,
    jobs: Api<Job>,
    deployments: Api<Deployment>,
    config: String,
    namespace: String,
    name: String,
    owner: Owner,
    retry: Duration,
    /// The last thing logged, so a steady state logs once.
    last: String,
}

impl Leader {
    fn say(&mut self, msg: String) {
        if self.last != msg {
            tracing::info!("{msg}");
            self.last = msg;
        }
    }

    async fn round(&mut self) -> Result<()> {
        let cm = self.configs.get(&self.config).await?;
        let data = cm.data.unwrap_or_default();
        let index = data.get("index").context("the ConfigMap names no index")?;
        let mut deployment: Deployment =
            serde_json::from_str(data.get("deployment.json").context("no deployment.json")?)
                .context("parse deployment.json")?;
        let job: Option<Job> = (data.get("job.json").map(|j| serde_json::from_str(j)))
            .transpose()
            .context("parse job.json")?;

        let sealed = self
            .store
            .exists(&format!("indexes/{index}/_COMPLETE"))
            .await?;
        if !sealed {
            return match job {
                Some(job) => self.build(index, job).await,
                None => {
                    self.say(format!("index {index} isn't published and building is off"));
                    Ok(())
                }
            };
        }

        let ours = ListParams::default().labels(&format!("{LABEL}={}", self.name));
        for old in self.jobs.list(&ours).await? {
            let name = old.metadata.name.unwrap_or_default();
            tracing::info!("deleting build job {name}: index {index} is published");
            self.jobs.delete(&name, &DeleteParams::background()).await?;
        }
        self.owner
            .claim(&mut deployment.metadata, "apps/Deployment", &self.namespace);
        let name = deployment
            .metadata
            .name
            .clone()
            .context("deployment name")?;
        let applied = self
            .deployments
            .patch(
                &name,
                &PatchParams::apply(MANAGER).force(),
                &Patch::Apply(&deployment),
            )
            .await?;
        let status = applied.status.unwrap_or_default();
        let want = applied.spec.and_then(|s| s.replicas);
        self.say(format!(
            "serving index {index}: {} of {} pods updated, {} ready",
            status.updated_replicas.unwrap_or(0),
            want.map_or("autoscaled".into(), |w| w.to_string()),
            status.ready_replicas.unwrap_or(0),
        ));
        Ok(())
    }

    async fn build(&mut self, index: &str, mut job: Job) -> Result<()> {
        let name = job.metadata.name.clone().context("job name")?;
        // One build at a time: the scratch volume is usually one ReadWriteOnce claim.
        let ours = ListParams::default().labels(&format!("{LABEL}={}", self.name));
        for other in self.jobs.list(&ours).await? {
            let other = other.metadata.name.unwrap_or_default();
            if other != name {
                tracing::info!("deleting build job {other}: the chart wants {name}");
                self.jobs
                    .delete(&other, &DeleteParams::background())
                    .await?;
            }
        }
        let existing = self.jobs.get_opt(&name).await?;
        match decide(existing.as_ref(), Timestamp::now(), self.retry) {
            Build::Create => {
                self.owner
                    .claim(&mut job.metadata, "batch/Job", &self.namespace);
                job.metadata
                    .labels
                    .get_or_insert_default()
                    .insert(LABEL.into(), self.name.clone());
                self.jobs.create(&PostParams::default(), &job).await?;
                self.say(format!("building index {index} in job {name}"));
            }
            Build::Running => self.say(format!("building index {index} in job {name}")),
            Build::Waiting(msg) => self.say(format!("build {name} {msg}")),
            Build::Replace(why) => {
                tracing::error!("build {name} {why}; replacing it");
                self.jobs.delete(&name, &DeleteParams::background()).await?;
            }
        }
        Ok(())
    }
}

async fn run() -> Result<()> {
    let namespace = required("NAMESPACE")?;
    let store = Store::open(&required("STORE_URL")?)?;
    store
        .list("indexes")
        .await
        .context("listing the bucket failed: it must exist before the leader starts")?;
    let kube = kube::Client::try_default().await?;
    let deployments: Api<Deployment> = Api::namespaced(kube.clone(), &namespace);
    let name = required("LEADER_DEPLOYMENT")?;
    let me = deployments
        .get(&name)
        .await
        .context("read own Deployment")?;
    let argo = (me.metadata.annotations.as_ref())
        .and_then(|a| a.get(TRACKING))
        .and_then(|t| t.split_once(':'))
        .map(|(app, _)| app.to_string());
    let owner = match argo {
        Some(app) => Owner::ArgoCd(app),
        None => Owner::Leader(OwnerReference {
            api_version: "apps/v1".into(),
            kind: "Deployment".into(),
            name: name.clone(),
            uid: me.metadata.uid.context("own Deployment has no uid")?,
            ..Default::default()
        }),
    };
    let mut lease = lease::Lease::new(
        Api::namespaced(kube.clone(), &namespace),
        required("LEASE_NAME")?,
        required("POD_NAME")?,
        Duration::from_secs(env("LEASE_SECONDS", 45)),
    );
    let mut leader = Leader {
        store,
        configs: Api::namespaced(kube.clone(), &namespace),
        jobs: Api::namespaced(kube, &namespace),
        deployments,
        config: required("CONFIG")?,
        namespace,
        name,
        owner,
        retry: Duration::from_secs(env("RETRY_SECONDS", 600)),
        last: String::new(),
    };
    let mut ticker = tokio::time::interval(Duration::from_secs(env("INTERVAL_SECONDS", 30)));
    loop {
        ticker.tick().await;
        if lease.acquire().await.is_none() {
            leader.last.clear();
            continue;
        }
        if let Err(e) = leader.round().await {
            tracing::warn!("round failed: {e:#}");
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let _sentry = sentry::init(sentry::ClientOptions {
        dsn: std::env::var("SENTRY_DSN")
            .ok()
            .filter(|s| !s.is_empty())
            .map(|s| s.parse())
            .transpose()?,
        environment: std::env::var("ENV").ok().map(std::borrow::Cow::Owned),
        release: sentry::release_name!(),
        ..Default::default()
    });
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    tokio::select! {
        r = run() => r,
        _ = term.recv() => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::api::batch::v1::{JobCondition, JobStatus};
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::Time;

    fn job(kind: Option<(&str, i64)>) -> Job {
        let conditions = kind.map(|(type_, at)| {
            vec![JobCondition {
                type_: type_.into(),
                status: "True".into(),
                message: Some("out of memory".into()),
                last_transition_time: Some(Time(Timestamp::from_second(at).unwrap())),
                ..Default::default()
            }]
        });
        Job {
            status: Some(JobStatus {
                conditions,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn claims_for_argo_cd_or_the_leader() {
        let mut meta = ObjectMeta {
            name: Some("og".into()),
            ..Default::default()
        };
        Owner::ArgoCd("app".into()).claim(&mut meta, "apps/Deployment", "ns");
        let a = meta.annotations.as_ref().unwrap();
        assert_eq!(a[TRACKING], "app:apps/Deployment:ns/og");
        assert_eq!(a["argocd.argoproj.io/sync-options"], "Prune=false");
        assert!(meta.owner_references.is_none());

        let mut meta = ObjectMeta::default();
        Owner::Leader(OwnerReference::default()).claim(&mut meta, "batch/Job", "ns");
        assert_eq!(meta.owner_references.map(|o| o.len()), Some(1));
        assert!(meta.annotations.is_none());
    }

    #[test]
    fn builds_once_and_retries_a_finished_job_after_the_delay() {
        let now = Timestamp::from_second(1000).unwrap();
        let retry = Duration::from_secs(600);
        assert_eq!(decide(None, now, retry), Build::Create);
        assert_eq!(decide(Some(&job(None)), now, retry), Build::Running);
        let failed = |at| decide(Some(&job(Some(("Failed", at)))), now, retry);
        assert!(
            matches!(failed(900), Build::Waiting(m) if m.contains("out of memory") && m.contains("500s"))
        );
        assert!(matches!(failed(400), Build::Replace(m) if m.contains("out of memory")));
        let done = decide(Some(&job(Some(("Complete", 0)))), now, retry);
        assert!(matches!(done, Build::Replace(m) if m.contains("without publishing")));
    }
}
