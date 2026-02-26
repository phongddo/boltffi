uniffi::setup_scaffolding!();

#[derive(uniffi::Record)]
pub struct Location {
    pub id: i64,
    pub lat: f64,
    pub lng: f64,
    pub rating: f64,
    pub review_count: i32,
    pub is_open: bool,
}

#[derive(uniffi::Record)]
pub struct Trade {
    pub id: i64,
    pub symbol_id: i32,
    pub price: f64,
    pub quantity: i64,
    pub bid: f64,
    pub ask: f64,
    pub volume: i64,
    pub timestamp: i64,
    pub is_buy: bool,
}

#[derive(uniffi::Record)]
pub struct Particle {
    pub id: i64,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub vx: f64,
    pub vy: f64,
    pub vz: f64,
    pub mass: f64,
    pub charge: f64,
    pub active: bool,
}

#[derive(uniffi::Record)]
pub struct SensorReading {
    pub sensor_id: i64,
    pub timestamp: i64,
    pub temperature: f64,
    pub humidity: f64,
    pub pressure: f64,
    pub light: f64,
    pub battery: f64,
    pub signal_strength: i32,
    pub is_valid: bool,
}

#[derive(uniffi::Record)]
pub struct UserProfile {
    pub id: i64,
    pub name: String,
    pub email: String,
    pub bio: String,
    pub age: i32,
    pub score: f64,
    pub tags: Vec<String>,
    pub scores: Vec<i32>,
    pub is_active: bool,
}

#[uniffi::export]
pub fn generate_user_profiles(count: i32) -> Vec<UserProfile> {
    (0..count as i64).map(|i| UserProfile {
        id: i,
        name: format!("User {}", i),
        email: format!("user{}@example.com", i),
        bio: format!("This is a bio for user {}. It contains some text to make it realistic.", i),
        age: 20 + (i % 50) as i32,
        score: (i as f64) * 1.5,
        tags: vec![format!("tag{}", i % 5), format!("category{}", i % 3), "common".to_string()],
        scores: vec![(i % 100) as i32, ((i + 10) % 100) as i32, ((i + 20) % 100) as i32],
        is_active: i % 2 == 0,
    }).collect()
}

#[uniffi::export]
pub fn sum_user_scores(users: Vec<UserProfile>) -> f64 {
    users.iter().map(|u| u.score).sum()
}

#[uniffi::export]
pub fn count_active_users(users: Vec<UserProfile>) -> i32 {
    users.iter().filter(|u| u.is_active).count() as i32
}

#[uniffi::export]
pub fn noop() {}

#[uniffi::export]
pub fn echo_i32(value: i32) -> i32 {
    value
}

#[uniffi::export]
pub fn echo_f64(value: f64) -> f64 {
    value
}

#[uniffi::export]
pub fn echo_string(value: String) -> String {
    value
}

#[uniffi::export]
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

#[uniffi::export]
pub async fn async_add(a: i32, b: i32) -> i32 {
    a + b
}

#[uniffi::export]
pub fn multiply(a: f64, b: f64) -> f64 {
    a * b
}

#[uniffi::export]
pub fn generate_string(size: i32) -> String {
    "x".repeat(size as usize)
}

#[uniffi::export]
pub fn generate_locations(count: i32) -> Vec<Location> {
    (0..count)
        .map(|i| Location {
            id: i as i64,
            lat: 37.7749 + (i as f64 * 0.001),
            lng: -122.4194 + (i as f64 * 0.001),
            rating: 3.0 + ((i % 20) as f64 * 0.1),
            review_count: 10 + (i * 5),
            is_open: i % 2 == 0,
        })
        .collect()
}

#[uniffi::export]
pub fn process_locations(locations: Vec<Location>) -> i32 {
    locations.len() as i32
}

#[uniffi::export]
pub fn sum_ratings(locations: Vec<Location>) -> f64 {
    locations.iter().map(|l| l.rating).sum()
}

#[uniffi::export]
pub fn generate_bytes(size: i32) -> Vec<u8> {
    vec![42u8; size as usize]
}

#[uniffi::export]
pub fn generate_i32_vec(count: i32) -> Vec<i32> {
    (0..count).collect()
}

#[uniffi::export]
pub fn sum_i32_vec(values: Vec<i32>) -> i64 {
    values.iter().map(|v| *v as i64).sum()
}

#[uniffi::export]
pub fn generate_trades(count: i32) -> Vec<Trade> {
    (0..count)
        .map(|i| Trade {
            id: i as i64,
            symbol_id: i % 500,
            price: 100.0 + (i as f64 * 0.01),
            quantity: (i as i64 % 1000) + 1,
            bid: 99.95 + (i as f64 * 0.01),
            ask: 100.05 + (i as f64 * 0.01),
            volume: (i as i64) * 1000,
            timestamp: 1700000000000 + (i as i64 * 1000),
            is_buy: i % 2 == 0,
        })
        .collect()
}

#[uniffi::export]
pub fn generate_particles(count: i32) -> Vec<Particle> {
    (0..count)
        .map(|i| Particle {
            id: i as i64,
            x: (i as f64) * 0.1,
            y: (i as f64) * 0.2,
            z: (i as f64) * 0.3,
            vx: (i as f64) * 0.01,
            vy: (i as f64) * 0.02,
            vz: (i as f64) * 0.03,
            mass: 1.0 + (i as f64 * 0.001),
            charge: if i % 2 == 0 { 1.0 } else { -1.0 },
            active: i % 10 != 0,
        })
        .collect()
}

#[uniffi::export]
pub fn generate_sensor_readings(count: i32) -> Vec<SensorReading> {
    (0..count)
        .map(|i| SensorReading {
            sensor_id: (i % 100) as i64,
            timestamp: 1700000000000 + (i as i64 * 100),
            temperature: 20.0 + ((i % 30) as f64),
            humidity: 40.0 + ((i % 40) as f64),
            pressure: 1013.25 + ((i % 20) as f64),
            light: (i % 1000) as f64,
            battery: 100.0 - ((i % 100) as f64),
            signal_strength: -50 - (i % 50),
            is_valid: i % 20 != 0,
        })
        .collect()
}

#[uniffi::export]
pub fn sum_trade_volumes(trades: Vec<Trade>) -> i64 {
    trades.iter().map(|t| t.volume).sum()
}

#[uniffi::export]
pub fn sum_particle_masses(particles: Vec<Particle>) -> f64 {
    particles.iter().map(|p| p.mass).sum()
}

#[uniffi::export]
pub fn avg_sensor_temperature(readings: Vec<SensorReading>) -> f64 {
    let sum: f64 = readings.iter().map(|r| r.temperature).sum();
    sum / readings.len() as f64
}

#[uniffi::export]
pub fn generate_f64_vec(count: i32) -> Vec<f64> {
    (0..count).map(|i| i as f64 * 0.1).collect()
}

#[uniffi::export]
pub fn sum_f64_vec(values: Vec<f64>) -> f64 {
    values.iter().sum()
}

#[uniffi::export]
pub fn inc_u64(value: u64) -> u64 {
    value + 1
}

#[derive(uniffi::Record)]
pub struct DataPoint {
    pub x: f64,
    pub y: f64,
    pub timestamp: i64,
}

#[derive(uniffi::Object)]
pub struct Counter {
    value: std::sync::Mutex<u64>,
}

#[uniffi::export]
impl Counter {
    #[uniffi::constructor]
    pub fn new() -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self {
            value: std::sync::Mutex::new(0),
        })
    }

    pub fn set(&self, value: u64) {
        *self.value.lock().unwrap() = value;
    }

    pub fn increment(&self) {
        *self.value.lock().unwrap() += 1;
    }

    pub fn get(&self) -> u64 {
        *self.value.lock().unwrap()
    }
}

#[derive(uniffi::Object)]
pub struct DataStore {
    items: std::sync::Mutex<Vec<DataPoint>>,
}

#[uniffi::export]
impl DataStore {
    #[uniffi::constructor]
    pub fn new() -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self {
            items: std::sync::Mutex::new(Vec::new()),
        })
    }

    pub fn add(&self, point: DataPoint) {
        self.items.lock().unwrap().push(point);
    }

    pub fn len(&self) -> u64 {
        self.items.lock().unwrap().len() as u64
    }

    pub fn is_empty(&self) -> bool {
        self.items.lock().unwrap().is_empty()
    }

    pub fn sum(&self) -> f64 {
        self.items.lock().unwrap().iter().map(|p| p.x + p.y).sum()
    }
}

#[derive(uniffi::Object)]
pub struct Accumulator {
    value: std::sync::Mutex<i64>,
}

#[uniffi::export]
impl Accumulator {
    #[uniffi::constructor]
    pub fn new() -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self {
            value: std::sync::Mutex::new(0),
        })
    }

    pub fn add(&self, amount: i64) {
        *self.value.lock().unwrap() += amount;
    }

    pub fn get(&self) -> i64 {
        *self.value.lock().unwrap()
    }

    pub fn reset(&self) {
        *self.value.lock().unwrap() = 0;
    }
}

#[derive(uniffi::Enum, Clone, Copy, PartialEq)]
pub enum Direction {
    North,
    East,
    South,
    West,
}

#[uniffi::export]
pub fn opposite_direction(dir: Direction) -> Direction {
    match dir {
        Direction::North => Direction::South,
        Direction::East => Direction::West,
        Direction::South => Direction::North,
        Direction::West => Direction::East,
    }
}

#[uniffi::export]
pub fn direction_to_degrees(dir: Direction) -> i32 {
    match dir {
        Direction::North => 0,
        Direction::East => 90,
        Direction::South => 180,
        Direction::West => 270,
    }
}

#[uniffi::export]
pub fn generate_directions(count: i32) -> Vec<Direction> {
    let variants = [Direction::North, Direction::East, Direction::South, Direction::West];
    (0..count as usize).map(|i| variants[i % 4]).collect()
}

#[uniffi::export]
pub fn count_north(directions: Vec<Direction>) -> i32 {
    directions.iter().filter(|d| **d == Direction::North).count() as i32
}

#[uniffi::export]
pub fn find_even(value: i32) -> Option<i32> {
    if value % 2 == 0 { Some(value) } else { None }
}

#[uniffi::export]
pub async fn compute_heavy(input: i32) -> i32 {
    std::thread::sleep(std::time::Duration::from_millis(100));
    input * 2
}

#[uniffi::export]
pub async fn async_make_string(value: i32) -> String {
    std::thread::sleep(std::time::Duration::from_millis(30));
    format!("Value is: {}", value)
}

#[uniffi::export]
pub async fn async_fetch_point(x: f64, y: f64) -> DataPoint {
    std::thread::sleep(std::time::Duration::from_millis(20));
    DataPoint {
        x,
        y,
        timestamp: 12345,
    }
}

#[uniffi::export]
pub async fn async_get_numbers(count: i32) -> Vec<i32> {
    std::thread::sleep(std::time::Duration::from_millis(20));
    (0..count).collect()
}

#[uniffi::export]
pub async fn async_find_value(needle: i32) -> Option<i32> {
    std::thread::sleep(std::time::Duration::from_millis(10));
    if needle > 0 { Some(needle * 100) } else { None }
}

#[uniffi::export(callback_interface)]
pub trait DataProvider: Send + Sync {
    fn get_count(&self) -> u32;
    fn get_item(&self, index: u32) -> DataPoint;
}

#[derive(uniffi::Object)]
pub struct DataConsumer {
    provider: std::sync::Mutex<Option<Box<dyn DataProvider>>>,
}

#[uniffi::export]
impl DataConsumer {
    #[uniffi::constructor]
    pub fn new() -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self {
            provider: std::sync::Mutex::new(None),
        })
    }

    pub fn set_provider(&self, provider: Box<dyn DataProvider>) {
        *self.provider.lock().unwrap() = Some(provider);
    }

    pub fn compute_sum(&self) -> u64 {
        let guard = self.provider.lock().unwrap();
        let Some(ref provider) = *guard else { return 0 };
        let count = provider.get_count();
        (0..count).map(|i| {
            let point = provider.get_item(i);
            (point.x + point.y) as u64
        }).sum()
    }
}


