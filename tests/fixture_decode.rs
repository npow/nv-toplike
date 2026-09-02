// SPDX-License-Identifier: Apache-2.0

use nv_toplike::model::{MetricQuality, MetricScope, MetricSource, SCHEMA_VERSION, Snapshot};

#[test]
fn sanitized_blackwell_fixture_decodes_with_metric_provenance() {
    let fixture = include_str!("fixtures/blackwell_workstation.json");
    let snapshot: Snapshot = serde_json::from_str(fixture).expect("fixture must decode");
    assert_eq!(snapshot.schema_version, SCHEMA_VERSION);
    assert_eq!(snapshot.devices.len(), 1);
    let device = &snapshot.devices[0];
    assert_eq!(device.device.id, "GPU-fixture-blackwell");
    let power = device
        .sample
        .power
        .power_watts
        .as_ref()
        .expect("fixture power");
    assert_eq!(power.source, MetricSource::Nvml);
    assert_eq!(power.scope, MetricScope::Device);
    assert_eq!(power.quality, MetricQuality::Direct);
    assert!(device.device.compute_units.is_none());
}
