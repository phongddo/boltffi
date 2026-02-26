import Benchmark
import BenchBoltFFI
import BenchUniffi
import Dispatch

print("DEBUG: Starting - about to initialize globals...")

let boltffiLocations1k = BenchBoltFFI.generateLocations(count: 1000)
print("DEBUG: boltffiLocations1k done")
let boltffiLocations10k = BenchBoltFFI.generateLocations(count: 10000)
let boltffiTrades1k = BenchBoltFFI.generateTrades(count: 1000)
let boltffiTrades10k = BenchBoltFFI.generateTrades(count: 10000)
let boltffiParticles1k = BenchBoltFFI.generateParticles(count: 1000)
let boltffiParticles10k = BenchBoltFFI.generateParticles(count: 10000)
let boltffiSensors1k = BenchBoltFFI.generateSensorReadings(count: 1000)
let boltffiSensors10k = BenchBoltFFI.generateSensorReadings(count: 10000)
let boltffiI32Vec10k = BenchBoltFFI.generateI32Vec(count: 10000)
let boltffiI32Vec100k = BenchBoltFFI.generateI32Vec(count: 100_000)
let boltffiF64Vec10k = BenchBoltFFI.generateF64Vec(count: 10000)

let uniffiLocations1k = BenchUniffi.generateLocations(count: 1000)
let uniffiLocations10k = BenchUniffi.generateLocations(count: 10000)
let uniffiTrades1k = BenchUniffi.generateTrades(count: 1000)
let uniffiTrades10k = BenchUniffi.generateTrades(count: 10000)
let uniffiParticles1k = BenchUniffi.generateParticles(count: 1000)
let uniffiParticles10k = BenchUniffi.generateParticles(count: 10000)
let uniffiSensors1k = BenchUniffi.generateSensorReadings(count: 1000)
let uniffiSensors10k = BenchUniffi.generateSensorReadings(count: 10000)
let uniffiI32Vec10k = BenchUniffi.generateI32Vec(count: 10000)
let uniffiI32Vec100k = BenchUniffi.generateI32Vec(count: 100_000)
let uniffiF64Vec10k = BenchUniffi.generateF64Vec(count: 10000)

benchmark("boltffi_noop") { BenchBoltFFI.noop() }
benchmark("uniffi_noop") { BenchUniffi.noop() }

benchmark("boltffi_echo_i32") { _ = BenchBoltFFI.echoI32(value: 42) }
benchmark("uniffi_echo_i32") { _ = BenchUniffi.echoI32(value: 42) }

benchmark("boltffi_add") { _ = BenchBoltFFI.add(a: 100, b: 200) }
benchmark("uniffi_add") { _ = BenchUniffi.add(a: 100, b: 200) }

benchmark("boltffi_multiply") { _ = BenchBoltFFI.multiply(a: 2.5, b: 4.0) }
benchmark("uniffi_multiply") { _ = BenchUniffi.multiply(a: 2.5, b: 4.0) }

benchmark("boltffi_inc_u64") {
    var arr: [UInt64] = [0]
    BenchBoltFFI.incU64(value: &arr)
    precondition(arr[0] == 1)
}

benchmark("uniffi_inc_u64") {
    var x: UInt64 = 0
    x = BenchUniffi.incU64(value: x)
    precondition(x == 1)
}

benchmark("boltffi_echo_string_small") { _ = BenchBoltFFI.echoString(value: "hello") }
benchmark("uniffi_echo_string_small") { _ = BenchUniffi.echoString(value: "hello") }

benchmark("boltffi_echo_string_1k") {
    _ = BenchBoltFFI.echoString(value: String(repeating: "x", count: 1000))
}

benchmark("uniffi_echo_string_1k") {
    _ = BenchUniffi.echoString(value: String(repeating: "x", count: 1000))
}

benchmark("boltffi_generate_locations_1k") { _ = BenchBoltFFI.generateLocations(count: 1000) }
benchmark("uniffi_generate_locations_1k") { _ = BenchUniffi.generateLocations(count: 1000) }

benchmark("boltffi_generate_locations_10k") { _ = BenchBoltFFI.generateLocations(count: 10000) }
benchmark("uniffi_generate_locations_10k") { _ = BenchUniffi.generateLocations(count: 10000) }

benchmark("boltffi_generate_trades_1k") { _ = BenchBoltFFI.generateTrades(count: 1000) }
benchmark("uniffi_generate_trades_1k") { _ = BenchUniffi.generateTrades(count: 1000) }

benchmark("boltffi_generate_trades_10k") { _ = BenchBoltFFI.generateTrades(count: 10000) }
benchmark("uniffi_generate_trades_10k") { _ = BenchUniffi.generateTrades(count: 10000) }

benchmark("boltffi_generate_particles_1k") { _ = BenchBoltFFI.generateParticles(count: 1000) }
benchmark("uniffi_generate_particles_1k") { _ = BenchUniffi.generateParticles(count: 1000) }

benchmark("boltffi_generate_particles_10k") { _ = BenchBoltFFI.generateParticles(count: 10000) }
benchmark("uniffi_generate_particles_10k") { _ = BenchUniffi.generateParticles(count: 10000) }

benchmark("boltffi_generate_sensors_1k") { _ = BenchBoltFFI.generateSensorReadings(count: 1000) }
benchmark("uniffi_generate_sensors_1k") { _ = BenchUniffi.generateSensorReadings(count: 1000) }

benchmark("boltffi_generate_sensors_10k") { _ = BenchBoltFFI.generateSensorReadings(count: 10000) }
benchmark("uniffi_generate_sensors_10k") { _ = BenchUniffi.generateSensorReadings(count: 10000) }

benchmark("boltffi_generate_i32_vec_10k") { _ = BenchBoltFFI.generateI32Vec(count: 10000) }
benchmark("uniffi_generate_i32_vec_10k") { _ = BenchUniffi.generateI32Vec(count: 10000) }

benchmark("boltffi_generate_i32_vec_100k") { _ = BenchBoltFFI.generateI32Vec(count: 100_000) }
benchmark("uniffi_generate_i32_vec_100k") { _ = BenchUniffi.generateI32Vec(count: 100_000) }

benchmark("boltffi_generate_f64_vec_10k") { _ = BenchBoltFFI.generateF64Vec(count: 10000) }
benchmark("uniffi_generate_f64_vec_10k") { _ = BenchUniffi.generateF64Vec(count: 10000) }

benchmark("boltffi_generate_bytes_64k") { _ = BenchBoltFFI.generateBytes(size: 65536) }
benchmark("uniffi_generate_bytes_64k") { _ = BenchUniffi.generateBytes(size: 65536) }

benchmark("boltffi_sum_ratings_1k") { _ = BenchBoltFFI.sumRatings(locations: boltffiLocations1k) }
benchmark("uniffi_sum_ratings_1k") { _ = BenchUniffi.sumRatings(locations: uniffiLocations1k) }

benchmark("boltffi_sum_ratings_10k") { _ = BenchBoltFFI.sumRatings(locations: boltffiLocations10k) }
benchmark("uniffi_sum_ratings_10k") { _ = BenchUniffi.sumRatings(locations: uniffiLocations10k) }

benchmark("boltffi_sum_trade_volumes_1k") { _ = BenchBoltFFI.sumTradeVolumes(trades: boltffiTrades1k) }
benchmark("uniffi_sum_trade_volumes_1k") { _ = BenchUniffi.sumTradeVolumes(trades: uniffiTrades1k) }

benchmark("boltffi_sum_trade_volumes_10k") { _ = BenchBoltFFI.sumTradeVolumes(trades: boltffiTrades10k) }
benchmark("uniffi_sum_trade_volumes_10k") {
    _ = BenchUniffi.sumTradeVolumes(trades: uniffiTrades10k)
}

benchmark("boltffi_sum_particle_masses_1k") {
    _ = BenchBoltFFI.sumParticleMasses(particles: boltffiParticles1k)
}

benchmark("uniffi_sum_particle_masses_1k") {
    _ = BenchUniffi.sumParticleMasses(particles: uniffiParticles1k)
}

benchmark("boltffi_sum_particle_masses_10k") {
    _ = BenchBoltFFI.sumParticleMasses(particles: boltffiParticles10k)
}

benchmark("uniffi_sum_particle_masses_10k") {
    _ = BenchUniffi.sumParticleMasses(particles: uniffiParticles10k)
}

benchmark("boltffi_avg_sensor_temp_1k") { _ = BenchBoltFFI.avgSensorTemperature(readings: boltffiSensors1k) }
benchmark("uniffi_avg_sensor_temp_1k") {
    _ = BenchUniffi.avgSensorTemperature(readings: uniffiSensors1k)
}

benchmark("boltffi_avg_sensor_temp_10k") {
    _ = BenchBoltFFI.avgSensorTemperature(readings: boltffiSensors10k)
}

benchmark("uniffi_avg_sensor_temp_10k") {
    _ = BenchUniffi.avgSensorTemperature(readings: uniffiSensors10k)
}

benchmark("boltffi_sum_i32_vec_10k") { _ = BenchBoltFFI.sumI32Vec(values: boltffiI32Vec10k) }
benchmark("uniffi_sum_i32_vec_10k") { _ = BenchUniffi.sumI32Vec(values: uniffiI32Vec10k) }

benchmark("boltffi_sum_i32_vec_100k") { _ = BenchBoltFFI.sumI32Vec(values: boltffiI32Vec100k) }
benchmark("uniffi_sum_i32_vec_100k") { _ = BenchUniffi.sumI32Vec(values: uniffiI32Vec100k) }

benchmark("boltffi_sum_f64_vec_10k") { _ = BenchBoltFFI.sumF64Vec(values: boltffiF64Vec10k) }
benchmark("uniffi_sum_f64_vec_10k") { _ = BenchUniffi.sumF64Vec(values: uniffiF64Vec10k) }

benchmark("boltffi_process_locations_1k") {
    _ = BenchBoltFFI.processLocations(locations: boltffiLocations1k)
}

benchmark("uniffi_process_locations_1k") {
    _ = BenchUniffi.processLocations(locations: uniffiLocations1k)
}

benchmark("boltffi_process_locations_10k") {
    _ = BenchBoltFFI.processLocations(locations: boltffiLocations10k)
}

benchmark("uniffi_process_locations_10k") {
    _ = BenchUniffi.processLocations(locations: uniffiLocations10k)
}

benchmark("boltffi_counter_increment_mutex") {
    let counter = BenchBoltFFI.Counter()
    for _ in 0 ..< 1000 {
        counter.increment()
    }
    precondition(counter.get() == 1000)
}

benchmark("uniffi_counter_increment_mutex") {
    let counter = BenchUniffi.Counter()
    for _ in 0 ..< 1000 {
        counter.increment()
    }
    precondition(counter.get() == 1000)
}

benchmark("boltffi_counter_increment_single_threaded") {
    let counter = BenchBoltFFI.CounterSingleThreaded()
    for _ in 0 ..< 1000 {
        counter.increment()
    }
    precondition(counter.get() == 1000)
}

benchmark("boltffi_datastore_add") {
    let store = BenchBoltFFI.DataStore()
    for i in 0 ..< 1000 {
        store.add(point: BenchBoltFFI.DataPoint(x: Double(i), y: Double(i) * 2.0, timestamp: Int64(i)))
    }
    precondition(store.len() == 1000)
}

benchmark("uniffi_datastore_add") {
    let store = BenchUniffi.DataStore()
    for i in 0 ..< 1000 {
        store.add(
            point: BenchUniffi.DataPoint(x: Double(i), y: Double(i) * 2.0, timestamp: Int64(i)))
    }
    precondition(store.len() == 1000)
}

benchmark("boltffi_accumulator_mutex") {
    let acc = BenchBoltFFI.Accumulator()
    for i: Int64 in 0 ..< 1000 {
        acc.add(amount: i)
    }
    _ = acc.get()
    acc.reset()
}

benchmark("uniffi_accumulator_mutex") {
    let acc = BenchUniffi.Accumulator()
    for i: Int64 in 0 ..< 1000 {
        acc.add(amount: i)
    }
    _ = acc.get()
    acc.reset()
}

benchmark("boltffi_accumulator_single_threaded") {
    let acc = BenchBoltFFI.AccumulatorSingleThreaded()
    for i: Int64 in 0 ..< 1000 {
        acc.add(amount: i)
    }
    _ = acc.get()
    acc.reset()
}

benchmark("boltffi_simple_enum") {
    _ = BenchBoltFFI.oppositeDirection(dir: .north)
    _ = BenchBoltFFI.directionToDegrees(dir: .east)
}

benchmark("uniffi_simple_enum") {
    _ = BenchUniffi.oppositeDirection(dir: .north)
    _ = BenchUniffi.directionToDegrees(dir: .east)
}

benchmark("boltffi_data_enum_input") {
    _ = BenchBoltFFI.getStatusProgress(status: .inProgress(progress: 50))
    _ = BenchBoltFFI.isStatusComplete(status: .completed(result: 100))
}

let boltffiDirections1k = BenchBoltFFI.generateDirections(count: 1000)
let boltffiDirections10k = BenchBoltFFI.generateDirections(count: 10000)
let uniffiDirections1k = BenchUniffi.generateDirections(count: 1000)
let uniffiDirections10k = BenchUniffi.generateDirections(count: 10000)

benchmark("boltffi_generate_directions_1k") { _ = BenchBoltFFI.generateDirections(count: 1000) }
benchmark("uniffi_generate_directions_1k") { _ = BenchUniffi.generateDirections(count: 1000) }

benchmark("boltffi_generate_directions_10k") { _ = BenchBoltFFI.generateDirections(count: 10000) }
benchmark("uniffi_generate_directions_10k") { _ = BenchUniffi.generateDirections(count: 10000) }

benchmark("boltffi_count_north_1k") { _ = BenchBoltFFI.countNorth(directions: boltffiDirections1k) }
benchmark("uniffi_count_north_1k") { _ = BenchUniffi.countNorth(directions: uniffiDirections1k) }

benchmark("boltffi_count_north_10k") { _ = BenchBoltFFI.countNorth(directions: boltffiDirections10k) }
benchmark("uniffi_count_north_10k") { _ = BenchUniffi.countNorth(directions: uniffiDirections10k) }

benchmark("boltffi_find_even") {
    for i: Int32 in 0 ..< 100 {
        _ = BenchBoltFFI.findEven(value: i)
    }
}

benchmark("uniffi_find_even") {
    for i: Int32 in 0 ..< 100 {
        _ = BenchUniffi.findEven(value: i)
    }
}

benchmark("boltffi_generate_user_profiles_100") {
    _ = BenchBoltFFI.generateUserProfiles(count: 100)
}

benchmark("uniffi_generate_user_profiles_100") {
    _ = BenchUniffi.generateUserProfiles(count: 100)
}

benchmark("boltffi_generate_user_profiles_1k") {
    _ = BenchBoltFFI.generateUserProfiles(count: 1000)
}

benchmark("uniffi_generate_user_profiles_1k") {
    _ = BenchUniffi.generateUserProfiles(count: 1000)
}

let boltffiUsers100 = BenchBoltFFI.generateUserProfiles(count: 100)
let boltffiUsers1k = BenchBoltFFI.generateUserProfiles(count: 1000)
let uniffiUsers100 = BenchUniffi.generateUserProfiles(count: 100)
let uniffiUsers1k = BenchUniffi.generateUserProfiles(count: 1000)

benchmark("boltffi_sum_user_scores_100") {
    _ = BenchBoltFFI.sumUserScores(users: boltffiUsers100)
}

benchmark("uniffi_sum_user_scores_100") {
    _ = BenchUniffi.sumUserScores(users: uniffiUsers100)
}

benchmark("boltffi_sum_user_scores_1k") {
    _ = BenchBoltFFI.sumUserScores(users: boltffiUsers1k)
}

benchmark("uniffi_sum_user_scores_1k") {
    _ = BenchUniffi.sumUserScores(users: uniffiUsers1k)
}

benchmark("boltffi_count_active_users_100") {
    _ = BenchBoltFFI.countActiveUsers(users: boltffiUsers100)
}

benchmark("uniffi_count_active_users_100") {
    _ = BenchUniffi.countActiveUsers(users: uniffiUsers100)
}

benchmark("boltffi_count_active_users_1k") {
    _ = BenchBoltFFI.countActiveUsers(users: boltffiUsers1k)
}

benchmark("uniffi_count_active_users_1k") {
    _ = BenchUniffi.countActiveUsers(users: uniffiUsers1k)
}

benchmark("boltffi_async_add") {
    let semaphore = DispatchSemaphore(value: 0)
    Task {
        _ = try! await BenchBoltFFI.asyncAdd(a: 100, b: 200)
        semaphore.signal()
    }
    semaphore.wait()
}

benchmark("uniffi_async_add") {
    let semaphore = DispatchSemaphore(value: 0)
    Task {
        _ = await BenchUniffi.asyncAdd(a: 100, b: 200)
        semaphore.signal()
    }
    semaphore.wait()
}

class BoltFFIDataProviderImpl: BenchBoltFFI.DataProvider {
    let points: [BenchBoltFFI.DataPoint]
    init(count: Int) {
        points = (0..<count).map { i in
            BenchBoltFFI.DataPoint(x: Double(i), y: Double(i) * 2.0, timestamp: Int64(i))
        }
    }
    func getCount() -> UInt32 { UInt32(points.count) }
    func getItem(index: UInt32) -> BenchBoltFFI.DataPoint { points[Int(index)] }
}

class UniffiDataProviderImpl: BenchUniffi.DataProvider {
    let points: [BenchUniffi.DataPoint]
    init(count: Int) {
        points = (0..<count).map { i in
            BenchUniffi.DataPoint(x: Double(i), y: Double(i) * 2.0, timestamp: Int64(i))
        }
    }
    func getCount() -> UInt32 { UInt32(points.count) }
    func getItem(index: UInt32) -> BenchUniffi.DataPoint { points[Int(index)] }
}

let boltffiProvider100 = BoltFFIDataProviderImpl(count: 100)
let boltffiProvider1k = BoltFFIDataProviderImpl(count: 1000)
let uniffiProvider100 = UniffiDataProviderImpl(count: 100)
let uniffiProvider1k = UniffiDataProviderImpl(count: 1000)

benchmark("boltffi_callback_100") {
    let consumer = BenchBoltFFI.DataConsumer()
    consumer.setProvider(provider: boltffiProvider100)
    _ = consumer.computeSum()
}

benchmark("uniffi_callback_100") {
    let consumer = BenchUniffi.DataConsumer()
    consumer.setProvider(provider: uniffiProvider100)
    _ = consumer.computeSum()
}

benchmark("boltffi_callback_1k") {
    let consumer = BenchBoltFFI.DataConsumer()
    consumer.setProvider(provider: boltffiProvider1k)
    _ = consumer.computeSum()
}

benchmark("uniffi_callback_1k") {
    let consumer = BenchUniffi.DataConsumer()
    consumer.setProvider(provider: uniffiProvider1k)
    _ = consumer.computeSum()
}

do {
    let even = BenchBoltFFI.findEven(value: 4)
    precondition(even == 4, "findEven(4) should return 4")
    let notEven = BenchBoltFFI.findEven(value: 3)
    precondition(notEven == nil, "findEven(3) should return nil")

    let posI64 = BenchBoltFFI.findPositiveI64(value: 100)
    precondition(posI64 == 100, "findPositiveI64(100) should return 100")
    let negI64 = BenchBoltFFI.findPositiveI64(value: -5)
    precondition(negI64 == nil, "findPositiveI64(-5) should return nil")

    let posF64 = BenchBoltFFI.findPositiveF64(value: 3.14)
    precondition(posF64 == 3.14, "findPositiveF64(3.14) should return 3.14")
    let negF64 = BenchBoltFFI.findPositiveF64(value: -1.0)
    precondition(negF64 == nil, "findPositiveF64(-1.0) should return nil")

    let name = BenchBoltFFI.findName(id: 1)
    precondition(name == "Name_1", "findName(1) should return Name_1")
    let noName = BenchBoltFFI.findName(id: -1)
    precondition(noName == nil, "findName(-1) should return nil")

    let loc = BenchBoltFFI.findLocation(id: 1)
    precondition(loc != nil && loc!.lat == 37.7749, "findLocation(1) should return location")
    let noLoc = BenchBoltFFI.findLocation(id: -1)
    precondition(noLoc == nil, "findLocation(-1) should return nil")

    let nums = BenchBoltFFI.findNumbers(count: 3)
    precondition(nums != nil && nums!.count == 3, "findNumbers(3) should return 3 numbers")
    let noNums = BenchBoltFFI.findNumbers(count: -1)
    precondition(noNums == nil, "findNumbers(-1) should return nil")

    let locs = BenchBoltFFI.findLocations(count: 2)
    precondition(locs != nil && locs!.count == 2, "findLocations(2) should return 2 locations")
    let noLocs = BenchBoltFFI.findLocations(count: -1)
    precondition(noLocs == nil, "findLocations(-1) should return nil")

    let dir = BenchBoltFFI.findDirection(id: 0)
    precondition(dir == .north, "findDirection(0) should return .north")
    let noDir = BenchBoltFFI.findDirection(id: -1)
    precondition(noDir == nil, "findDirection(-1) should return nil")

    let names = BenchBoltFFI.findNames(count: 3)
    precondition(names != nil && names!.count == 3, "findNames(3) should return 3 names")
    precondition(names![0] == "Name_0", "findNames(3)[0] should be 'Name_0'")
    let noNames = BenchBoltFFI.findNames(count: -1)
    precondition(noNames == nil, "findNames(-1) should return nil")

    let dirs = BenchBoltFFI.findDirections(count: 4)
    precondition(dirs != nil && dirs!.count == 4, "findDirections(4) should return 4 directions")
    precondition(dirs![0] == .north, "findDirections(4)[0] should be .north")
    precondition(dirs![1] == .east, "findDirections(4)[1] should be .east")
    let noDirs = BenchBoltFFI.findDirections(count: -1)
    precondition(noDirs == nil, "findDirections(-1) should return nil")

    let apiRes = BenchBoltFFI.findApiResult(code: 0)
    precondition(apiRes != nil, "findApiResult(0) should return Some")
    let noApiRes = BenchBoltFFI.findApiResult(code: -1)
    precondition(noApiRes == nil, "findApiResult(-1) should return nil")

    print("Option tests passed")

    let pendingProgress = BenchBoltFFI.getStatusProgress(status: .pending)
    precondition(pendingProgress == 0, "Pending status should have progress 0")
    
    let inProgressProgress = BenchBoltFFI.getStatusProgress(status: .inProgress(progress: 75))
    precondition(inProgressProgress == 75, "InProgress(75) should return 75")
    
    let completedProgress = BenchBoltFFI.getStatusProgress(status: .completed(result: 100))
    precondition(completedProgress == 100, "Completed(100) should return 100")
    
    let failedProgress = BenchBoltFFI.getStatusProgress(status: .failed(errorCode: -5, retryCount: 3))
    precondition(failedProgress == -5, "Failed(-5, 3) should return -5")
    
    let isPendingComplete = BenchBoltFFI.isStatusComplete(status: .pending)
    precondition(isPendingComplete == false, "Pending should not be complete")
    
    let isCompletedComplete = BenchBoltFFI.isStatusComplete(status: .completed(result: 42))
    precondition(isCompletedComplete == true, "Completed should be complete")
    
    print("Data enum tests passed")
}

let sem = DispatchSemaphore(value: 0)
Task {
    let r1 = try! await BenchBoltFFI.tryComputeAsync(value: 5)
    precondition(r1 == 10, "tryComputeAsync failed")

    do { _ = try await BenchBoltFFI.tryComputeAsync(value: -1) } catch is BenchBoltFFI.ComputeError {}

    let r2 = try! await BenchBoltFFI.fetchData(id: 5)
    precondition(r2 == 50, "fetchData failed")

    do { _ = try await BenchBoltFFI.fetchData(id: -1) } catch is BenchBoltFFI.FfiError {}

    let nums = try! await BenchBoltFFI.asyncGetNumbers(count: 5)
    precondition(nums.count == 5, "asyncGetNumbers should return 5 numbers")
    precondition(nums == [0, 1, 2, 3, 4], "asyncGetNumbers should return [0,1,2,3,4]")

    let store = BenchBoltFFI.DataStore()
    let len = try! await store.asyncLen()
    precondition(len == 0, "asyncLen on empty store should be 0")

    do {
        _ = try await store.asyncSum()
        preconditionFailure("asyncSum on empty should throw")
    } catch {
        precondition(error is BenchBoltFFI.FfiError, "asyncSum should throw FfiError")
    }

    print("Async class method tests passed")

    let emptyStore = BenchBoltFFI.DataStore()
    precondition(emptyStore.len() == 0, "new() should create empty store")
    print("  DataStore() - empty store: OK")

    let sampleStore = BenchBoltFFI.DataStore.withSampleData()
    precondition(sampleStore.len() == 3, "withSampleData() should have 3 items")
    print("  DataStore.withSampleData() - 3 items: OK")

    let capacityStore = BenchBoltFFI.DataStore(withCapacity: 100)
    precondition(capacityStore.len() == 0, "withCapacity() should be empty")
    print("  DataStore(withCapacity: 100) - empty with capacity: OK")

    let pointStore = BenchBoltFFI.DataStore(withInitialPoint: 1.0, y: 2.0, timestamp: 100)
    precondition(pointStore.len() == 1, "withInitialPoint() should have 1 item")
    var foundPoint = false
    pointStore.foreach { point in
        precondition(point.x == 1.0, "x should be 1.0")
        precondition(point.y == 2.0, "y should be 2.0")
        precondition(point.timestamp == 100, "timestamp should be 100")
        foundPoint = true
    }
    precondition(foundPoint, "should have found the point")
    print("  DataStore(withInitialPoint: 1.0, y: 2.0, timestamp: 100) - 1 item verified: OK")
    print("Factory constructor tests passed")

    var callCount = 0
    var sumX = 0.0
    sampleStore.foreach { point in
        callCount += 1
        sumX += point.x
    }
    precondition(callCount == 3, "foreach should be called 3 times: \(callCount)")
    precondition(sumX == 9.0, "sumX should be 9.0: \(sumX)")
    print("Closure callback tests passed (3 items, sumX=9.0)")

    print("Async Result tests passed")

    let successPoint = BenchBoltFFI.DataPoint(x: 1.5, y: 2.5, timestamp: 999)
    let successResponse = BenchBoltFFI.createSuccessResponse(requestId: 42, point: successPoint)
    precondition(successResponse.requestId == 42, "request_id should be 42")
    precondition(BenchBoltFFI.isResponseSuccess(response: successResponse) == true, "success response should be success")
    if case .success(let point) = successResponse.result {
        precondition(point.x == 1.5, "point.x should be 1.5")
        precondition(point.y == 2.5, "point.y should be 2.5")
        precondition(point.timestamp == 999, "point.timestamp should be 999")
    } else {
        preconditionFailure("success response should have .success result")
    }
    let successValue = BenchBoltFFI.getResponseValue(response: successResponse)
    precondition(successValue != nil && successValue!.x == 1.5, "getResponseValue should return the point")

    let errorResponse = BenchBoltFFI.createErrorResponse(requestId: 100, error: .invalidInput(-999))
    precondition(errorResponse.requestId == 100, "request_id should be 100")
    precondition(BenchBoltFFI.isResponseSuccess(response: errorResponse) == false, "error response should not be success")
    if case .failure(let err) = errorResponse.result {
        if case .invalidInput(let errValue) = err {
            precondition(errValue == -999, "error value should be -999")
        } else {
            preconditionFailure("error should be invalidInput")
        }
    } else {
        preconditionFailure("error response should have .failure result")
    }
    let errorValue = BenchBoltFFI.getResponseValue(response: errorResponse)
    precondition(errorValue == nil, "getResponseValue on error should return nil")

    print("Result field tests passed")
    sem.signal()
}

sem.wait()

Benchmark.main()
