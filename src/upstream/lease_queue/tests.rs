use super::*;
use std::sync::mpsc;
use std::thread;

#[test]
fn queue_is_fifo() {
    let first = enqueue("fifo", 4).expect("first ticket");
    let second = enqueue("fifo", 4).expect("second ticket");
    assert_eq!(first.position().ticket, 0);
    assert_eq!(second.position().ticket, 1);
    first
        .wait_until_head(Instant::now() + Duration::from_millis(50))
        .expect("first is head");
    first.complete();
    second
        .wait_until_head(Instant::now() + Duration::from_millis(50))
        .expect("second advances");
    second.complete();
}

#[test]
fn cancelled_head_wakes_next_waiter() {
    let first = enqueue("cancel", 4).expect("first ticket");
    let second = enqueue("cancel", 4).expect("second ticket");
    drop(first);
    second
        .wait_until_head(Instant::now() + Duration::from_millis(50))
        .expect("cancelled head skipped");
    second.complete();
}

#[test]
fn queue_depth_is_bounded() {
    let first = enqueue("bounded", 1).expect("first ticket");
    let error = match enqueue("bounded", 1) {
        Ok(_) => panic!("second ticket should be rejected"),
        Err(error) => error,
    };
    assert!(matches!(error, LeaseQueueError::Full { maximum: 1, .. }));
    first.complete();
}

#[test]
fn ticket_finalization_repairs_and_reports_a_poisoned_lane() {
    let ticket = enqueue("poison-recovery", 2).expect("ticket");
    let lane = Arc::clone(&ticket.lane);
    let poison_lane = Arc::clone(&lane);
    let handle = thread::spawn(move || {
        let _state = poison_lane.state.lock().expect("lane state lock");
        panic!("intentional queue-state poison");
    });
    assert!(handle.join().is_err());
    assert!(lane.state.is_poisoned());

    ticket.complete();

    assert!(!lane.state.is_poisoned());
    let state = lane.state.lock().expect("repaired lane state");
    assert_eq!(state.waiting, 0);
    assert_eq!(state.serving_ticket, state.next_ticket);
}

#[test]
fn release_notification_interrupts_retry_wait() {
    let ticket = enqueue("notify", 1).expect("ticket");
    ticket
        .wait_until_head(Instant::now() + Duration::from_millis(50))
        .expect("head");
    let (sender, receiver) = mpsc::channel();
    let handle = thread::spawn(move || {
        let started = Instant::now();
        let result = ticket.wait_for_retry(
            Duration::from_secs(1),
            Instant::now() + Duration::from_secs(1),
        );
        sender.send((result, started.elapsed())).expect("send");
    });
    thread::sleep(Duration::from_millis(20));
    notify_all_lanes();
    let (result, elapsed) = receiver.recv().expect("receive");
    assert!(result.is_ok());
    assert!(elapsed < Duration::from_millis(500));
    handle.join().expect("join");
}
