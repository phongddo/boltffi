import Foundation


public final class Counter {
    let handle: OpaquePointer

    private init(handle: OpaquePointer) {
        self.handle = handle
    }

    public convenience init() {
        let ptr = mffi_counter_new()!
        self.init(handle: ptr)
    }

    deinit {
        _ = mffi_counter_free(handle)
    }

    public func set(value: UInt64) {
        mffi_counter_set(handle, value)
    }

    public func increment() {
        mffi_counter_increment(handle)
    }

    public func get() -> UInt64 {
        return mffi_counter_get(handle)
    }
}


public final class Accumulator {
    let handle: OpaquePointer

    private init(handle: OpaquePointer) {
        self.handle = handle
    }

    public convenience init() {
        let ptr = mffi_accumulator_new()!
        self.init(handle: ptr)
    }

    deinit {
        _ = mffi_accumulator_free(handle)
    }

    public func add(amount: Int64) {
        mffi_accumulator_add(handle, amount)
    }

    public func get() -> Int64 {
        return mffi_accumulator_get(handle)
    }

    public func reset() {
        mffi_accumulator_reset(handle)
    }
}


public final class SensorMonitor {
    let handle: OpaquePointer

    private init(handle: OpaquePointer) {
        self.handle = handle
    }

    public convenience init() {
        let ptr = mffi_sensormonitor_new()!
        self.init(handle: ptr)
    }

    deinit {
        _ = mffi_sensormonitor_free(handle)
    }

    public func emitReading(sensorId: Int32, timestampMs: Int64, value: Double) {
        mffi_sensormonitor_emit_reading(handle, sensorId, timestampMs, value)
    }

    public func subscriberCount() -> UInt {
        return mffi_sensormonitor_subscriber_count(handle)
    }

    public func readings() -> AsyncStream<SensorReading> {
        AsyncStream<SensorReading> { continuation in
    let subscription = mffi_sensormonitor_readings(self.handle)
    
    continuation.onTermination = { @Sendable _ in
        mffi_sensormonitor_readings_unsubscribe(subscription)
        mffi_sensormonitor_readings_free(subscription)
    }
    
    Task {
        let buffer = UnsafeMutablePointer<SensorReading>.allocate(capacity: 1)
        defer { buffer.deallocate() }
        
        while true {
            let waitResult = mffi_sensormonitor_readings_wait(subscription, 1000)
            
            if waitResult < 0 { break }
            if waitResult == 0 { continue }
            
            let count = mffi_sensormonitor_readings_pop_batch(subscription, buffer, 1)
            if count > 0 {
                continuation.yield(buffer.pointee)
            }
        }
        
        continuation.finish()
    }
}
    }
}

