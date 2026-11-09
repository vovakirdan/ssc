//! Opt-in transport counters only. Never serialize the underlying WebRTC stats report.
use super::enabled;
use crate::peer::state::{self, lock};
use serde::Serialize;
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use webrtc::{
    peer_connection::RTCPeerConnection,
    stats::{StatsReport, StatsReportType},
};

#[derive(Clone, Serialize)]
pub struct PairGroup {
    local_type: &'static str,
    remote_type: &'static str,
    state: &'static str,
    nominated: bool,
    pairs: u32,
    requests_sent: Option<u64>,
    responses_received: Option<u64>,
}
#[derive(Serialize)]
pub struct TransportDiagnostics {
    generation: u64,
    peer_connection: &'static str,
    ice: &'static str,
    ice_transport: &'static str,
    dtls: &'static str,
    sctp: &'static str,
    stats_available: bool,
    counters_available: bool,
    pairs: Vec<PairGroup>,
    turn_wire: webrtc::ice::turn_diagnostics::WireSnapshot,
}
fn candidate_type(value: &str) -> &'static str {
    match value {
        "host" => "host",
        "srflx" => "srflx",
        "prflx" => "prflx",
        "relay" => "relay",
        _ => "unknown",
    }
}
fn pair_state(value: &str) -> &'static str {
    match value {
        "waiting" => "waiting",
        "in-progress" => "in_progress",
        "in_progress" => "in_progress",
        "failed" => "failed",
        "succeeded" => "succeeded",
        "frozen" => "frozen",
        _ => "unknown",
    }
}
fn transport_state(value: &str) -> &'static str {
    match value {
        "new" => "new",
        "checking" => "checking",
        "connecting" => "connecting",
        "connected" => "connected",
        "completed" => "completed",
        "disconnected" => "disconnected",
        "failed" => "failed",
        "closed" => "closed",
        _ => "unknown",
    }
}
fn summarize(stats: &StatsReport) -> Vec<PairGroup> {
    let kind = |id: &str| {
        stats
            .reports
            .get(id)
            .map_or("unknown", |report| match report {
                StatsReportType::LocalCandidate(candidate)
                | StatsReportType::RemoteCandidate(candidate) => {
                    candidate_type(&candidate.candidate_type.to_string())
                }
                _ => "unknown",
            })
    };
    let mut groups = BTreeMap::new();
    for report in stats.reports.values() {
        if let StatsReportType::CandidatePair(pair) = report {
            let key = (
                kind(&pair.local_candidate_id),
                kind(&pair.remote_candidate_id),
                pair_state(&pair.state.to_string()),
                pair.nominated,
            );
            let group = groups.entry(key).or_insert(PairGroup {
                local_type: key.0,
                remote_type: key.1,
                state: key.2,
                nominated: key.3,
                pairs: 0,
                requests_sent: None,
                responses_received: None,
            });
            group.pairs = group.pairs.saturating_add(1);
        }
    }
    groups.into_values().collect()
}
async fn collect_if_enabled(
    pc: &RTCPeerConnection,
    generation: u64,
    is_enabled: impl Fn() -> bool,
) -> Option<TransportDiagnostics> {
    if !is_enabled() {
        return None;
    }
    let stats = tokio::time::timeout(Duration::from_secs(1), pc.get_stats())
        .await
        .ok();
    if !is_enabled() {
        return None;
    }
    let dtls = pc.dtls_transport();
    Some(TransportDiagnostics {
        generation,
        peer_connection: transport_state(&pc.connection_state().to_string()),
        ice: transport_state(&pc.ice_connection_state().to_string()),
        ice_transport: transport_state(&dtls.ice_transport().state().to_string()),
        dtls: transport_state(&dtls.state().to_string()),
        sctp: transport_state(&pc.sctp().state().to_string()),
        stats_available: stats.is_some(),
        // The pinned 0.13 ICE provider populates only pair IDs/state/nominated;
        // its request/response fields are default zero, not measured counters.
        counters_available: false,
        pairs: stats.as_ref().map(summarize).unwrap_or_default(),
        turn_wire: webrtc::ice::turn_diagnostics::snapshot(),
    })
}
#[tauri::command]
pub async fn get_transport_diagnostics() -> Option<TransportDiagnostics> {
    if !enabled() {
        return None;
    }
    let session = state::current().ok()?;
    if !session.active() {
        return None;
    }
    let pc = lock(&session.data).pc.clone()?;
    let result = collect_if_enabled(&pc, session.generation, enabled).await;
    if !session.active()
        || !state::current()
            .ok()
            .is_some_and(|current| Arc::ptr_eq(&current, &session))
    {
        return None;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::HashMap,
        sync::atomic::{AtomicUsize, Ordering},
    };
    use webrtc::{
        api::APIBuilder,
        ice::{
            agent::agent_stats::CandidatePairStats,
            candidate::{CandidatePairState, CandidateType},
            network_type::NetworkType,
        },
        peer_connection::configuration::RTCConfiguration,
        stats::{ICECandidatePairStats, ICECandidateStats, RTCStatsType},
    };
    fn candidate(id: &str, kind: CandidateType) -> ICECandidateStats {
        ICECandidateStats {
            timestamp: tokio::time::Instant::now(),
            stats_type: RTCStatsType::LocalCandidate,
            id: id.into(),
            candidate_type: kind,
            deleted: false,
            ip: "203.0.113.123".into(),
            network_type: NetworkType::Udp4,
            port: 65432,
            priority: 17,
            relay_protocol: "udp".into(),
            url: "turn:SECRET_PASSWORD@PRIVATE_SERVER".into(),
        }
    }
    #[test]
    fn aggregates_pair_types_and_states_without_exporting_any_raw_stats_fields() {
        let mut reports = HashMap::new();
        reports.insert(
            "PRIVATE_LOCAL_ID".into(),
            StatsReportType::LocalCandidate(candidate("PRIVATE_LOCAL_ID", CandidateType::Relay)),
        );
        reports.insert(
            "PRIVATE_REMOTE_ID".into(),
            StatsReportType::RemoteCandidate(candidate("PRIVATE_REMOTE_ID", CandidateType::Host)),
        );
        for number in 0..2 {
            reports.insert(
                format!("PRIVATE_PAIR_{number}"),
                StatsReportType::CandidatePair(ICECandidatePairStats::from(CandidatePairStats {
                    local_candidate_id: "PRIVATE_LOCAL_ID".into(),
                    remote_candidate_id: "PRIVATE_REMOTE_ID".into(),
                    state: CandidatePairState::Succeeded,
                    nominated: true,
                    requests_sent: u64::MAX,
                    responses_received: u64::MAX,
                    ..Default::default()
                })),
            );
        }
        let groups = summarize(&StatsReport { reports });
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].pairs, 2);
        assert_eq!(groups[0].local_type, "relay");
        assert_eq!(groups[0].remote_type, "host");
        assert_eq!(groups[0].state, "succeeded");
        let json = serde_json::to_string(&groups).unwrap();
        for forbidden in [
            "PRIVATE",
            "SECRET",
            "203.0.113",
            "65432",
            "timestamp",
            "fingerprint",
            "candidateId",
            "18446744073709551615",
        ] {
            assert!(!json.contains(forbidden));
        }
        assert!(json.contains("\"requests_sent\":null"));
        assert!(json.contains("\"responses_received\":null"));
        assert_eq!(candidate_type("PRIVATE_HOST"), "unknown");
        assert_eq!(pair_state("PASSWORD"), "unknown");
        assert_eq!(transport_state("SDP"), "unknown");
    }
    #[tokio::test]
    async fn disabled_diagnostics_return_nothing_and_enabled_reads_actual_webrtc_states() {
        let pc = APIBuilder::new()
            .build()
            .new_peer_connection(RTCConfiguration::default())
            .await
            .unwrap();
        assert!(collect_if_enabled(&pc, 17, || false).await.is_none());
        let live = collect_if_enabled(&pc, 17, || true).await.unwrap();
        assert_eq!(live.generation, 17);
        assert!(live.stats_available);
        assert!(!live.counters_available);
        assert_eq!(live.peer_connection, "new");
        assert!(live.pairs.is_empty());
        let enabled_checks = AtomicUsize::new(0);
        assert!(
            collect_if_enabled(&pc, 17, || enabled_checks.fetch_add(1, Ordering::SeqCst)
                == 0)
            .await
            .is_none()
        );
        pc.close().await.unwrap();
    }
}
