//! Capability Resolver: select a backend from the registry for probed hardware.

use std::collections::HashMap;

use crate::manifest::registry::{Backend, SovereignRegistry};
use crate::resolution::prober::ProbedHardware;
use serde_json::Value;
use tracing::{info, warn};

pub struct CapabilityResolver;

impl CapabilityResolver {
    /// Resolve the optimal backend based on hardware identity and registry rules.
    pub fn resolve(
        hardware: &ProbedHardware,
        registry: &SovereignRegistry,
    ) -> Option<Backend> {
        info!(
            "Resolving optimal backend for {} on {}",
            hardware.gpu_vendor, hardware.os
        );

        for rule in &registry.routing_rules {
            if let Some(condition) = &rule.condition {
                if Self::matches(hardware, condition) {
                    if let Some(backend_id) = &rule.use_backend {
                        if let Some(backend) =
                            registry.backends.iter().find(|backend| backend.id == *backend_id)
                        {
                            info!("Matched routing rule; selected backend: {}", backend_id);
                            return Some(backend.clone());
                        }
                    }
                }
            }
        }

        warn!("No direct routing rule matched; using fallback chain");

        for fallback_id in &registry.global_policies.fallback_order {
            if let Some(backend) = registry.backends.iter().find(|backend| {
                backend.engine.variant == *fallback_id
                    && (backend.platform.os == hardware.os
                        || backend.platform.os == "universal")
            }) {
                info!("Selected fallback backend: {}", backend.id);
                return Some(backend.clone());
            }
        }

        None
    }

    fn matches(
        hardware: &ProbedHardware,
        condition: &HashMap<String, Value>,
    ) -> bool {
        if let Some(expected_vendor) = condition.get("hardware").and_then(Value::as_str) {
            if expected_vendor != hardware.gpu_vendor.as_str() {
                return false;
            }
        }

        if let Some(expected_os) = condition.get("os").and_then(Value::as_str) {
            if expected_os != hardware.os.as_str() && expected_os != "universal" {
                return false;
            }
        }

        if let Some(requirement) = condition.get("cuda_version").and_then(Value::as_str) {
            let Some(detected_version) = hardware.cuda_version.as_deref() else {
                return false;
            };

            if !Self::version_matches(detected_version, requirement) {
                return false;
            }
        }

        true
    }

    fn version_matches(detected: &str, requirement: &str) -> bool {
        let requirement = requirement.trim();

        if let Some(minimum) = requirement.strip_prefix(">=") {
            return Self::version_parts(detected) >= Self::version_parts(minimum.trim());
        }

        if let Some(maximum) = requirement.strip_prefix("<=") {
            return Self::version_parts(detected) <= Self::version_parts(maximum.trim());
        }

        if let Some(minimum_exclusive) = requirement.strip_prefix('>') {
            return Self::version_parts(detected)
                > Self::version_parts(minimum_exclusive.trim());
        }

        if let Some(maximum_exclusive) = requirement.strip_prefix('<') {
            return Self::version_parts(detected)
                < Self::version_parts(maximum_exclusive.trim());
        }

        Self::version_parts(detected) == Self::version_parts(requirement)
    }

    fn version_parts(version: &str) -> Vec<u64> {
        version
            .split(|character: char| !character.is_ascii_digit())
            .filter(|part| !part.is_empty())
            .filter_map(|part| part.parse::<u64>().ok())
            .collect()
    }

    /// Resolve the artifact URL by substituting registry placeholders.
    pub fn resolve_artifact_url(
        backend: &Backend,
        registry: &SovereignRegistry,
        version: &str,
    ) -> String {
        let Some(artifact) = backend.artifacts.first() else {
            warn!("Backend {} contains no artifacts", backend.id);
            return String::new();
        };

        let Some(template) = artifact.download_url_template.as_deref() else {
            warn!("Backend {} artifact has no download URL template", backend.id);
            return String::new();
        };

        template
            .replace(
                "{BASE_URL}",
                registry.global_policies.artifact_base_url.as_str(),
            )
            .replace("{VERSION}", version)
            .replace("{os}", std::env::consts::OS)
            .replace("{arch}", std::env::consts::ARCH)
    }
}
