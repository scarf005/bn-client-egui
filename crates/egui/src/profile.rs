//! Scope timings of the frames puffin recorded, for the live harness and the present probe.
use std::collections::HashMap;

/// Prints per scope name the inclusive time per recorded frame and the time of a single call, largest first.
pub fn scopes(phase: &str, view: &puffin::GlobalFrameView) {
    fn walk(
        names: &puffin::ScopeCollection,
        stream: &puffin::Stream,
        reader: puffin::Reader<'_>,
        total: &mut HashMap<String, Vec<u64>>,
    ) {
        for scope in reader.flatten() {
            let name = names
                .fetch_by_id(&scope.id)
                .map_or("?".to_string(), |d| d.name().to_string());
            total
                .entry(name)
                .or_default()
                .push(scope.record.duration_ns as u64);
            if let Ok(children) = puffin::Reader::with_offset(stream, scope.child_begin_position) {
                walk(names, stream, children, total);
            }
        }
    }
    let mut total = HashMap::new();
    let guard = view.lock();
    let frames = guard.all_uniq().cloned().collect::<Vec<_>>();
    for data in &frames {
        let Ok(unpacked) = data.unpacked();
        for info in unpacked.thread_streams.values() {
            walk(
                guard.scope_collection(),
                &info.stream,
                puffin::Reader::from_start(&info.stream),
                &mut total,
            );
        }
    }
    let mut rows = total.into_iter().collect::<Vec<_>>();
    rows.sort_by_key(|(_, ns)| std::cmp::Reverse(ns.iter().sum::<u64>()));
    let n = frames.len().max(1) as f64;
    println!(
        "PERF scopes {phase}: {} frames; ms per frame, calls per frame, ms per call p50/p95/max",
        frames.len()
    );
    for (name, mut ns) in rows.into_iter().take(30) {
        ns.sort_unstable();
        let at = |p: f64| ns[((ns.len() - 1) as f64 * p).round() as usize] as f64 / 1e6;
        println!(
            "PERF   {:9.3} ms {:7.1} calls {:8.3}/{:8.3}/{:8.3}  {name}",
            ns.iter().sum::<u64>() as f64 / 1e6 / n,
            ns.len() as f64 / n,
            at(0.5),
            at(0.95),
            at(1.0)
        );
    }
}
