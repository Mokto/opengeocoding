//! Leader election on a coordination.k8s.io Lease: one replica acts, the other stands by and
//! takes over when it is gone.
use std::time::{Duration, Instant};

use k8s_openapi::api::coordination::v1::{Lease as LeaseObject, LeaseSpec};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{MicroTime, ObjectMeta};
use k8s_openapi::jiff::Timestamp;
use kube::api::{Api, PostParams};

pub struct Lease {
    api: Api<LeaseObject>,
    name: String,
    identity: String,
    duration: Duration,
    /// The lease's resourceVersion and when this pod first saw it. Another holder's
    /// lease expires `duration` after it last changed on this pod's clock, so nodes
    /// whose clocks disagree can't both think they hold it.
    observed: Option<(String, Instant)>,
    held: bool,
}

impl Lease {
    pub fn new(api: Api<LeaseObject>, name: String, identity: String, duration: Duration) -> Self {
        Self {
            api,
            name,
            identity,
            duration,
            observed: None,
            held: false,
        }
    }

    /// Takes or renews the lease. While this pod holds it, returns until when it may
    /// act: half the lease, so a slow round stops before a standby could take over.
    pub async fn acquire(&mut self) -> Option<Instant> {
        let start = Instant::now();
        let held = match self.try_acquire(start).await {
            Ok(held) => held,
            Err(kube::Error::Api(s)) if s.is_conflict() || s.is_already_exists() => false,
            Err(e) => {
                tracing::warn!("lease {}: {e}", self.name);
                false
            }
        };
        if held != self.held {
            let role = if held { "leading" } else { "standing by" };
            tracing::info!("{} {role}", self.identity);
            self.held = held;
        }
        held.then(|| start + self.duration / 2)
    }

    async fn try_acquire(&mut self, now: Instant) -> kube::Result<bool> {
        let time = MicroTime(Timestamp::now());
        let seconds = self.duration.as_secs() as i32;
        let Some(mut lease) = self.api.get_opt(&self.name).await? else {
            let lease = LeaseObject {
                metadata: ObjectMeta {
                    name: Some(self.name.clone()),
                    ..Default::default()
                },
                spec: Some(LeaseSpec {
                    holder_identity: Some(self.identity.clone()),
                    lease_duration_seconds: Some(seconds),
                    acquire_time: Some(time.clone()),
                    renew_time: Some(time),
                    ..Default::default()
                }),
            };
            self.api.create(&PostParams::default(), &lease).await?;
            return Ok(true);
        };
        let version = lease.metadata.resource_version.clone().unwrap_or_default();
        if self.observed.as_ref().is_none_or(|(v, _)| *v != version) {
            self.observed = Some((version, now));
        }
        let expired = self
            .observed
            .as_ref()
            .is_some_and(|(_, seen)| now.duration_since(*seen) >= self.duration);
        let spec = lease.spec.get_or_insert_default();
        let mine = spec.holder_identity.as_deref() == Some(self.identity.as_str());
        if !mine && !expired {
            return Ok(false);
        }
        if !mine {
            spec.holder_identity = Some(self.identity.clone());
            spec.acquire_time = Some(time.clone());
            spec.lease_transitions = Some(spec.lease_transitions.unwrap_or(0) + 1);
        }
        spec.lease_duration_seconds = Some(seconds);
        spec.renew_time = Some(time);
        // Sent with the resourceVersion read above: if another replica wrote the lease
        // since, this is a conflict instead of a second holder.
        self.api
            .replace(&self.name, &PostParams::default(), &lease)
            .await?;
        Ok(true)
    }
}
