use buffer_ring::RingBuffer;

#[test]
fn ringbuffer_push_and_recent() {
    let mut rb: RingBuffer<i32> = RingBuffer::new(5);
    rb.push(1);
    rb.push(2);
    rb.push(3);
    assert_eq!(rb.recent(2), vec![2, 3]);
    assert_eq!(rb.recent(10), vec![1, 2, 3]);
    assert_eq!(rb.len(), 3);
}

#[test]
fn ringbuffer_overflow() {
    let mut rb: RingBuffer<i32> = RingBuffer::new(3);
    rb.push(1);
    rb.push(2);
    rb.push(3);
    rb.push(4);
    rb.push(5);
    assert_eq!(rb.len(), 3);
    assert_eq!(rb.all(), vec![3, 4, 5]);
}

#[test]
fn ringbuffer_extend() {
    let mut rb: RingBuffer<i32> = RingBuffer::new(5);
    rb.extend(&[1, 2, 3, 4, 5]);
    assert_eq!(rb.all(), vec![1, 2, 3, 4, 5]);
    rb.extend(&[6, 7]);
    assert_eq!(rb.all(), vec![3, 4, 5, 6, 7]);
}

#[test]
fn ringbuffer_empty() {
    let rb: RingBuffer<i32> = RingBuffer::new(5);
    assert!(rb.is_empty());
    assert_eq!(rb.recent(10), Vec::<i32>::new());
}

#[test]
fn ringbuffer_clear() {
    let mut rb: RingBuffer<i32> = RingBuffer::new(5);
    rb.push(1);
    rb.push(2);
    rb.clear();
    assert!(rb.is_empty());
}

#[test]
fn ringbuffer_resize_smaller() {
    let mut rb: RingBuffer<i32> = RingBuffer::new(5);
    rb.extend(&[1, 2, 3, 4, 5]);
    rb.resize(3);
    assert_eq!(rb.capacity(), 3);
    assert_eq!(rb.all(), vec![3, 4, 5]);
}

#[test]
fn ringbuffer_resize_larger() {
    let mut rb: RingBuffer<i32> = RingBuffer::new(3);
    rb.extend(&[1, 2, 3]);
    rb.resize(5);
    assert_eq!(rb.capacity(), 5);
    assert_eq!(rb.all(), vec![1, 2, 3]);
}

#[test]
fn ringbuffer_capacity_one() {
    let mut rb: RingBuffer<i32> = RingBuffer::new(1);
    rb.push(1);
    rb.push(2);
    rb.push(3);
    assert_eq!(rb.len(), 1);
    assert_eq!(rb.all(), vec![3]);
}

#[test]
fn ringbuffer_capacity_zero_clamps_to_one() {
    let mut rb: RingBuffer<i32> = RingBuffer::new(0);
    assert_eq!(rb.capacity(), 1);
    rb.push(42);
    assert_eq!(rb.all(), vec![42]);
}

#[test]
fn ringbuffer_resize_zero_clamps_to_one() {
    let mut rb: RingBuffer<i32> = RingBuffer::new(5);
    rb.extend(&[1, 2, 3, 4, 5]);
    rb.resize(0);
    assert_eq!(rb.capacity(), 1);
    assert_eq!(rb.all(), vec![5]);
}

#[test]
fn ringbuffer_clear_then_push_resets_head() {
    let mut rb: RingBuffer<i32> = RingBuffer::new(3);
    rb.extend(&[1, 2, 3]);
    rb.clear();
    assert_eq!(rb.len(), 0);
    rb.push(99);
    assert_eq!(rb.all(), vec![99]);
}

#[test]
fn ringbuffer_clone_preserves_state() {
    let mut rb: RingBuffer<i32> = RingBuffer::new(3);
    rb.extend(&[1, 2]);
    let cloned = rb.clone();
    assert_eq!(cloned.all(), vec![1, 2]);
    assert_eq!(cloned.capacity(), 3);
}

#[test]
fn ringbuffer_recent_smaller_than_zero_clamped() {
    let mut rb: RingBuffer<i32> = RingBuffer::new(3);
    rb.push(1);
    assert_eq!(rb.recent(0), Vec::<i32>::new());
}