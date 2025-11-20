#![allow(unused)]

pub mod handle;
pub mod pending;
pub mod ringbuffer;
pub mod rustfuture;
pub mod safety;
pub mod status;
pub mod subscription;
pub mod types;

pub use handle::HandleBox;
pub use riff_macros::{
    Data, FfiType, data, export, ffi_class, ffi_export, ffi_stream, ffi_trait, name, skip,
};
pub use pending::{CancellationToken, PendingHandle};
pub use ringbuffer::SpscRingBuffer;
pub use rustfuture::{
    RustFuture, RustFutureContinuationCallback, RustFutureHandle, RustFuturePoll,
};
pub use safety::catch_ffi_panic;
pub use status::{FfiStatus, clear_last_error, set_last_error, take_last_error};
pub use subscription::{
    EventSubscription, StreamContinuationCallback, StreamPollResult, StreamProducer,
    SubscriptionHandle, WaitResult,
};
pub use types::{FfiBuf, FfiOption, FfiSlice, FfiString};

#[unsafe(no_mangle)]
pub extern "C" fn riff_free_buf_i32(buf: FfiBuf<i32>) {
    drop(buf);
}

#[unsafe(no_mangle)]
pub extern "C" fn riff_option_i32_is_some(opt: FfiOption<i32>) -> bool {
    opt.is_some()
}

unsafe fn read_input_str<'a>(ptr: *const u8, len: usize) -> Option<&'a str> {
    if ptr.is_null() {
        return None;
    }
    let bytes = core::slice::from_raw_parts(ptr, len);
    core::str::from_utf8(bytes).ok()
}

pub const VERSION_MAJOR: u32 = 0;
pub const VERSION_MINOR: u32 = 1;
pub const VERSION_PATCH: u32 = 0;

#[unsafe(no_mangle)]
pub extern "C" fn riff_version_major() -> u32 {
    VERSION_MAJOR
}

#[unsafe(no_mangle)]
pub extern "C" fn riff_version_minor() -> u32 {
    VERSION_MINOR
}

#[unsafe(no_mangle)]
pub extern "C" fn riff_version_patch() -> u32 {
    VERSION_PATCH
}

#[unsafe(no_mangle)]
pub extern "C" fn riff_free_buf_u8(buf: FfiBuf<u8>) {
    drop(buf);
}

#[unsafe(no_mangle)]
pub extern "C" fn riff_free_string(string: FfiString) {
    drop(string);
}

#[unsafe(no_mangle)]
pub extern "C" fn riff_last_error_message(out: *mut FfiString) -> FfiStatus {
    if out.is_null() {
        return FfiStatus::NULL_POINTER;
    }

    match take_last_error() {
        Some(message) => {
            unsafe { *out = FfiString::from(message) };
            FfiStatus::OK
        }
        None => {
            unsafe { *out = FfiString::from("") };
            FfiStatus::OK
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn riff_clear_last_error() {
    clear_last_error();
}

pub fn fail_with_error(status: FfiStatus, message: impl Into<String>) -> FfiStatus {
    set_last_error(message);
    status
}

#[ffi_export]
pub fn greeting(name: &str) -> String {
    format!("Hello, {}!", name)
}

#[ffi_export]
pub fn concat(first: &str, second: &str) -> String {
    format!("{}{}", first, second)
}

#[ffi_export]
pub fn reverse_string(input: String) -> String {
    input.chars().rev().collect()
}

#[ffi_export]
pub fn copy_bytes(src: &[u8], dst: &mut [u8]) -> usize {
    let len = src.len().min(dst.len());
    dst[..len].copy_from_slice(&src[..len]);
    len
}

pub struct Counter {
    value: u64,
}

#[ffi_class]
impl Counter {
    pub fn new() -> Self {
        Self { value: 0 }
    }

    pub fn set(&mut self, value: u64) {
        self.value = value;
    }

    pub fn increment(&mut self) {
        self.value += 1;
    }

    pub fn get(&self) -> u64 {
        self.value
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DataPoint {
    pub x: f64,
    pub y: f64,
    pub timestamp: i64,
}

pub struct DataStore {
    items: Vec<DataPoint>,
}

#[ffi_class]
impl DataStore {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    pub fn add(&mut self, point: DataPoint) {
        self.items.push(point);
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    #[skip]
    pub fn internal_debug(&self) -> String {
        format!("DataStore with {} items", self.items.len())
    }

    pub fn copy_into(&self, dst: &mut [DataPoint]) -> usize {
        let len = self.items.len().min(dst.len());
        dst[..len].copy_from_slice(&self.items[..len]);
        len
    }

    pub fn foreach(&self, mut callback: impl FnMut(DataPoint)) {
        self.items.iter().for_each(|p| callback(*p));
    }

    pub fn sum(&self) -> f64 {
        self.items.iter().map(|p| p.x + p.y).sum()
    }
}

#[ffi_export]
pub fn add_numbers(first: i32, second: i32) -> i32 {
    first + second
}

#[ffi_export]
pub fn multiply_floats(first: f64, second: f64) -> f64 {
    first * second
}

#[ffi_export]
pub fn make_greeting(name: &str) -> String {
    format!("Hello, {}!", name)
}

#[ffi_export]
pub fn safe_divide(numerator: i32, denominator: i32) -> Result<i32, &'static str> {
    if denominator == 0 {
        Err("division by zero")
    } else {
        Ok(numerator / denominator)
    }
}

#[ffi_export]
pub fn generate_sequence(count: i32) -> Vec<i32> {
    (0..count).collect()
}

#[ffi_export]
pub fn foreach_range(start: i32, end: i32, mut callback: impl FnMut(i32)) {
    (start..end).for_each(|i| callback(i));
}

pub struct Accumulator {
    value: i64,
}

#[ffi_class]
impl Accumulator {
    pub fn new() -> Self {
        Self { value: 0 }
    }

    pub fn add(&mut self, amount: i64) {
        self.value += amount;
    }

    pub fn get(&self) -> i64 {
        self.value
    }

    pub fn reset(&mut self) {
        self.value = 0;
    }
}

#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Direction {
    North = 0,
    East = 1,
    South = 2,
    West = 3,
}

#[ffi_export]
pub fn opposite_direction(dir: Direction) -> Direction {
    match dir {
        Direction::North => Direction::South,
        Direction::East => Direction::West,
        Direction::South => Direction::North,
        Direction::West => Direction::East,
    }
}

#[ffi_export]
pub fn direction_to_degrees(dir: Direction) -> i32 {
    match dir {
        Direction::North => 0,
        Direction::East => 90,
        Direction::South => 180,
        Direction::West => 270,
    }
}

#[ffi_export]
pub fn find_even(value: i32) -> Option<i32> {
    if value % 2 == 0 { Some(value) } else { None }
}

#[repr(C, i32)]
#[derive(Clone, Copy, Debug)]
pub enum ApiResult {
    Success = 0,
    ErrorCode(i32) = 1,
    ErrorWithData { code: i32, detail: i32 } = 2,
}

#[ffi_export]
pub fn process_value(value: i32) -> ApiResult {
    if value > 0 {
        ApiResult::Success
    } else if value == 0 {
        ApiResult::ErrorCode(-1)
    } else {
        ApiResult::ErrorWithData {
            code: value,
            detail: value * 2,
        }
    }
}

#[ffi_export]
pub fn api_result_is_success(result: ApiResult) -> bool {
    matches!(result, ApiResult::Success)
}

#[ffi_export]
pub async fn compute_heavy(input: i32) -> i32 {
    std::thread::sleep(std::time::Duration::from_millis(100));
    input * 2
}

#[ffi_export]
pub async fn fetch_data(id: i32) -> Result<i32, &'static str> {
    std::thread::sleep(std::time::Duration::from_millis(50));
    if id > 0 {
        Ok(id * 10)
    } else {
        Err("invalid id")
    }
}

#[ffi_export]
pub async fn async_make_string(value: i32) -> String {
    std::thread::sleep(std::time::Duration::from_millis(30));
    format!("Value is: {}", value)
}

#[ffi_export]
pub async fn async_fetch_point(x: f64, y: f64) -> DataPoint {
    std::thread::sleep(std::time::Duration::from_millis(20));
    DataPoint {
        x,
        y,
        timestamp: 12345,
    }
}

#[ffi_export]
pub async fn async_get_numbers(count: i32) -> Vec<i32> {
    std::thread::sleep(std::time::Duration::from_millis(20));
    (0..count).collect()
}

#[ffi_export]
pub async fn async_find_value(needle: i32) -> Option<i32> {
    std::thread::sleep(std::time::Duration::from_millis(10));
    if needle > 0 { Some(needle * 100) } else { None }
}

#[ffi_export]
pub async fn async_greeting(name: &str) -> String {
    std::thread::sleep(std::time::Duration::from_millis(10));
    format!("Hello, {}!", name)
}

#[ffi_export]
pub async fn async_fetch_numbers(id: i32) -> Result<Vec<i32>, &'static str> {
    std::thread::sleep(std::time::Duration::from_millis(20));
    if id > 0 {
        Ok((0..id).map(|x| x * 2).collect())
    } else {
        Err("invalid id")
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct TestEvent {
    pub event_id: i32,
    pub value: i64,
}

#[unsafe(no_mangle)]
pub extern "C" fn riff_test_events_subscribe(capacity: usize) -> SubscriptionHandle {
    subscription::subscription_new::<TestEvent>(capacity)
}

#[unsafe(no_mangle)]
pub extern "C" fn riff_test_events_push(
    handle: SubscriptionHandle,
    event_id: i32,
    value: i64,
) -> bool {
    let event = TestEvent { event_id, value };
    unsafe { subscription::subscription_push(handle, event) }
}

#[unsafe(no_mangle)]
pub extern "C" fn riff_test_events_pop_batch(
    handle: SubscriptionHandle,
    output_ptr: *mut TestEvent,
    output_capacity: usize,
) -> usize {
    unsafe { subscription::subscription_pop_batch(handle, output_ptr, output_capacity) }
}

#[unsafe(no_mangle)]
pub extern "C" fn riff_test_events_wait(
    handle: SubscriptionHandle,
    timeout_milliseconds: u32,
) -> i32 {
    unsafe { subscription::subscription_wait::<TestEvent>(handle, timeout_milliseconds) }
}

#[unsafe(no_mangle)]
pub extern "C" fn riff_test_events_unsubscribe(handle: SubscriptionHandle) {
    unsafe { subscription::subscription_unsubscribe::<TestEvent>(handle) }
}

#[unsafe(no_mangle)]
pub extern "C" fn riff_test_events_free(handle: SubscriptionHandle) {
    unsafe { subscription::subscription_free::<TestEvent>(handle) }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct SensorReading {
    pub sensor_id: i32,
    pub timestamp_ms: i64,
    pub value: f64,
}

pub struct SensorMonitor {
    readings_producer: StreamProducer<SensorReading>,
}

#[ffi_class]
impl SensorMonitor {
    pub fn new() -> Self {
        Self {
            readings_producer: StreamProducer::new(512),
        }
    }

    #[ffi_stream(item = SensorReading)]
    pub fn readings(&self) -> std::sync::Arc<EventSubscription<SensorReading>> {
        self.readings_producer.subscribe()
    }

    pub fn emit_reading(&self, sensor_id: i32, timestamp_ms: i64, value: f64) {
        self.readings_producer.push(SensorReading {
            sensor_id,
            timestamp_ms,
            value,
        });
    }

    pub fn subscriber_count(&self) -> usize {
        self.readings_producer.subscriber_count()
    }
}

#[ffi_trait]
pub trait DataProvider {
    fn get_count(&self) -> u32;
    fn get_item(&self, index: u32) -> DataPoint;
}

#[ffi_trait]
pub trait AsyncDataFetcher {
    async fn fetch_value(&self, key: u32) -> u64;
}

pub struct DataConsumer {
    provider: Option<Box<dyn DataProvider>>,
}

#[ffi_class]
impl DataConsumer {
    pub fn new() -> Self {
        Self { provider: None }
    }

    pub fn set_provider(&mut self, provider: Box<dyn DataProvider>) {
        self.provider = Some(provider);
    }

    pub fn compute_sum(&self) -> u64 {
        let Some(ref provider) = self.provider else {
            return 0;
        };
        let count = provider.get_count();
        (0..count)
            .map(|i| {
                let point = provider.get_item(i);
                (point.x + point.y) as u64
            })
            .sum()
    }
}
