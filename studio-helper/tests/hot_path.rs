use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};

use dashboard_helper_protocol::*;
use studio_helper::jobs::JobStore;
use studio_helper::jobs::logs::LogInput;
struct Counting;
static BYTES: AtomicU64 = AtomicU64::new(0);
// Dedicated single-test binary: count allocation volume, not allocator timing noise.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(
        &self,
        l: Layout,
    ) -> *mut u8 {
        BYTES.fetch_add(l.size() as u64, Ordering::Relaxed);
        unsafe { System.alloc(l) }
    }

    unsafe fn dealloc(
        &self,
        p: *mut u8,
        l: Layout,
    ) {
        unsafe { System.dealloc(p, l) }
    }

    unsafe fn realloc(
        &self,
        p: *mut u8,
        l: Layout,
        n: usize,
    ) -> *mut u8 {
        BYTES.fetch_add(n as u64, Ordering::Relaxed);
        unsafe { System.realloc(p, l, n) }
    }
}
#[global_allocator]
static ALLOC: Counting = Counting;
#[test]
fn full_history_ingestion_does_not_copy_retained_records() {
    let mut s = JobStore::new(SessionId::new("a".repeat(32)).unwrap());
    let id = JobId::new("b".repeat(32)).unwrap();
    s.start(id.clone(), OperationKind::DeviceScan, None).unwrap();
    let input = LogInput {
        stage: StageId::Probe(ProbeStage::DeviceDetection),
        source: Source::Stdout,
        text: "x".repeat(256),
        progress: false,
    };
    for _ in 0..1100 {
        s.append(&id, input.clone()).unwrap();
    }
    for _ in 0..256 {
        let _ = s.enqueue(&id, input.clone()).unwrap();
    }
    let before = BYTES.load(Ordering::Relaxed);
    for _ in 0..1000 {
        s.append(&id, input.clone()).unwrap();
    }
    let allocated = BYTES.load(Ordering::Relaxed) - before;
    println!("1000 appends with 1024 retained and 256 queued records allocated {allocated} bytes");
    assert!(allocated < 32_000_000, "bounded append must not clone retained history");
    for text in [
        "\"".repeat(1024),
        "\\".repeat(1024),
        "\0".repeat(1024),
        "🦀".repeat(256),
    ] {
        let mut input = input.clone();
        input.text = text;
        let before = BYTES.load(Ordering::Relaxed);
        for _ in 0..100 {
            s.append(&id, input.clone()).unwrap();
        }
        let allocated = BYTES.load(Ordering::Relaxed) - before;
        println!(
            "100 maximum chunks starting {:?}: {allocated} cumulative allocated bytes",
            input.text.chars().next()
        );
        assert!(
            allocated < 4_000_000,
            "fitting must not repeatedly serialize shrinking text"
        );
        let after = s
            .poll(
                &id,
                PollRequest {
                    after: u64::MAX,
                    limit: 1,
                },
            )
            .unwrap()
            .next
            - 1;
        // A separate append starts at this cursor; every fragment must preserve exact text.
        s.append(&id, input.clone()).unwrap();
        let batch = s.poll(&id, PollRequest { after, limit: 128 }).unwrap();
        let joined: String = batch
            .records
            .iter()
            .map(|event| {
                assert!(encode(event).unwrap().len() <= 1024);
                event.text.as_str()
            })
            .collect();
        assert_eq!(joined, input.text);
    }
}
