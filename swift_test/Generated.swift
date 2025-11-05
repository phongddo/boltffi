import Foundation

public func greeting(name: String) -> String {
    var result = FfiString(ptr: nil, len: 0, cap: 0)
    return name.withCString { namePtr in 
    let status = mffi_greeting(namePtr, UInt(name.utf8.count), &result)
    defer { mffi_free_string(result) }
    return String(data: Data(bytes: result.ptr!, count: Int(result.len)), encoding: .utf8)!
    }
}

public func concat(first: String, second: String) -> String {
    var result = FfiString(ptr: nil, len: 0, cap: 0)
    return first.withCString { firstPtr in second.withCString { secondPtr in 
    let status = mffi_concat(firstPtr, UInt(first.utf8.count), secondPtr, UInt(second.utf8.count), &result)
    defer { mffi_free_string(result) }
    return String(data: Data(bytes: result.ptr!, count: Int(result.len)), encoding: .utf8)!
    }
    }
}

public func reverseString(input: String) -> String {
    var result = FfiString(ptr: nil, len: 0, cap: 0)
    return input.withCString { inputPtr in 
    let status = mffi_reverse_string(inputPtr, UInt(input.utf8.count), &result)
    defer { mffi_free_string(result) }
    return String(data: Data(bytes: result.ptr!, count: Int(result.len)), encoding: .utf8)!
    }
}

public func addNumbers(first: Int32, second: Int32) -> Int32 {
    return mffi_add_numbers(first, second)
}

public func multiplyFloats(first: Double, second: Double) -> Double {
    return mffi_multiply_floats(first, second)
}

public func makeGreeting(name: String) -> String {
    var result = FfiString(ptr: nil, len: 0, cap: 0)
    return name.withCString { namePtr in 
    let status = mffi_make_greeting(namePtr, UInt(name.utf8.count), &result)
    defer { mffi_free_string(result) }
    return String(data: Data(bytes: result.ptr!, count: Int(result.len)), encoding: .utf8)!
    }
}

public func foreachRange(start: Int32, end: Int32, callback: @escaping (Int32) -> Void) {
    typealias ForeachRangeCallbackFn = (Int32) -> Void
    class ForeachRangeCallbackBox { let fn_: ForeachRangeCallbackFn; init(_ fn_: @escaping ForeachRangeCallbackFn) { self.fn_ = fn_ } }
    let callbackBox = ForeachRangeCallbackBox(callback)
    let callbackPtr = Unmanaged.passRetained(callbackBox).toOpaque()
    let callbackTrampoline: @convention(c) (UnsafeMutableRawPointer?, Int32) -> Void = { ud, val in
        Unmanaged<ForeachRangeCallbackBox>.fromOpaque(ud!).takeUnretainedValue().fn_(val)
    }
    _ = mffi_foreach_range(start, end, callbackTrampoline, callbackPtr)
    Unmanaged<ForeachRangeCallbackBox>.fromOpaque(callbackPtr).release()
}

public func oppositeDirection(dir: Direction) -> Direction {
    return mffi_opposite_direction(dir)
}

public func directionToDegrees(dir: Direction) -> Int32 {
    return mffi_direction_to_degrees(dir)
}

public func processValue(value: Int32) -> ApiResult {
    return mffi_process_value(value)
}

public func apiResultIsSuccess(result: ApiResult) -> Bool {
    return mffi_api_result_is_success(result)
}

public func computeHeavy(input: Int32) async throws -> Int32 {
    let futureHandle = mffi_compute_heavy(input)
    
    class FutureContext {
        let handle: RustFutureHandle?
        let continuation: CheckedContinuation<Int32, Error>
        
        init(handle: RustFutureHandle?, continuation: CheckedContinuation<Int32, Error>) {
            self.handle = handle
            self.continuation = continuation
        }
    }
    
    return try await withTaskCancellationHandler {
        try await withCheckedThrowingContinuation { continuation in
            let context = FutureContext(handle: futureHandle, continuation: continuation)
            
            func poll(ctx: FutureContext) {
                mffi_compute_heavy_poll(ctx.handle, UInt64(UInt(bitPattern: Unmanaged.passRetained(ctx).toOpaque()))) { callbackData, pollResult in
                    let ctx = Unmanaged<FutureContext>.fromOpaque(UnsafeRawPointer(bitPattern: UInt(callbackData))!).takeRetainedValue()
                    if pollResult == 0 {
                        var status = FfiStatus()
                        let result = mffi_compute_heavy_complete(ctx.handle, &status)
                        mffi_compute_heavy_free(ctx.handle)
                        ctx.continuation.resume(returning: result)
                    } else {
                        poll(ctx: ctx)
                    }
                }
            }
            poll(ctx: context)
        }
    } onCancel: {
        mffi_compute_heavy_cancel(futureHandle)
        mffi_compute_heavy_free(futureHandle)
    }
}

public func asyncMakeString(value: Int32) async throws -> String {
    let futureHandle = mffi_async_make_string(value)
    
    class FutureContext {
        let handle: RustFutureHandle?
        let continuation: CheckedContinuation<String, Error>
        
        init(handle: RustFutureHandle?, continuation: CheckedContinuation<String, Error>) {
            self.handle = handle
            self.continuation = continuation
        }
    }
    
    return try await withTaskCancellationHandler {
        try await withCheckedThrowingContinuation { continuation in
            let context = FutureContext(handle: futureHandle, continuation: continuation)
            
            func poll(ctx: FutureContext) {
                mffi_async_make_string_poll(ctx.handle, UInt64(UInt(bitPattern: Unmanaged.passRetained(ctx).toOpaque()))) { callbackData, pollResult in
                    let ctx = Unmanaged<FutureContext>.fromOpaque(UnsafeRawPointer(bitPattern: UInt(callbackData))!).takeRetainedValue()
                    if pollResult == 0 {
                        var status = FfiStatus()
                        let ffiStr = mffi_async_make_string_complete(ctx.handle, &status)
                        let str = String(data: Data(bytes: ffiStr.ptr!, count: Int(ffiStr.len)), encoding: .utf8)!
                        mffi_free_string(ffiStr)
                        mffi_async_make_string_free(ctx.handle)
                        ctx.continuation.resume(returning: str)
                    } else {
                        poll(ctx: ctx)
                    }
                }
            }
            poll(ctx: context)
        }
    } onCancel: {
        mffi_async_make_string_cancel(futureHandle)
        mffi_async_make_string_free(futureHandle)
    }
}

public func asyncFetchPoint(x: Double, y: Double) async throws -> DataPoint {
    let futureHandle = mffi_async_fetch_point(x, y)
    
    class FutureContext {
        let handle: RustFutureHandle?
        let continuation: CheckedContinuation<DataPoint, Error>
        
        init(handle: RustFutureHandle?, continuation: CheckedContinuation<DataPoint, Error>) {
            self.handle = handle
            self.continuation = continuation
        }
    }
    
    return try await withTaskCancellationHandler {
        try await withCheckedThrowingContinuation { continuation in
            let context = FutureContext(handle: futureHandle, continuation: continuation)
            
            func poll(ctx: FutureContext) {
                mffi_async_fetch_point_poll(ctx.handle, UInt64(UInt(bitPattern: Unmanaged.passRetained(ctx).toOpaque()))) { callbackData, pollResult in
                    let ctx = Unmanaged<FutureContext>.fromOpaque(UnsafeRawPointer(bitPattern: UInt(callbackData))!).takeRetainedValue()
                    if pollResult == 0 {
                        var status = FfiStatus()
                        let result = mffi_async_fetch_point_complete(ctx.handle, &status)
                        mffi_async_fetch_point_free(ctx.handle)
                        ctx.continuation.resume(returning: result)
                    } else {
                        poll(ctx: ctx)
                    }
                }
            }
            poll(ctx: context)
        }
    } onCancel: {
        mffi_async_fetch_point_cancel(futureHandle)
        mffi_async_fetch_point_free(futureHandle)
    }
}

public func asyncGreeting(name: String) async throws -> String {
    let futureHandle = mffi_async_greeting(name, UInt(name.utf8.count))
    
    class FutureContext {
        let handle: RustFutureHandle?
        let continuation: CheckedContinuation<String, Error>
        
        init(handle: RustFutureHandle?, continuation: CheckedContinuation<String, Error>) {
            self.handle = handle
            self.continuation = continuation
        }
    }
    
    return try await withTaskCancellationHandler {
        try await withCheckedThrowingContinuation { continuation in
            let context = FutureContext(handle: futureHandle, continuation: continuation)
            
            func poll(ctx: FutureContext) {
                mffi_async_greeting_poll(ctx.handle, UInt64(UInt(bitPattern: Unmanaged.passRetained(ctx).toOpaque()))) { callbackData, pollResult in
                    let ctx = Unmanaged<FutureContext>.fromOpaque(UnsafeRawPointer(bitPattern: UInt(callbackData))!).takeRetainedValue()
                    if pollResult == 0 {
                        var status = FfiStatus()
                        let ffiStr = mffi_async_greeting_complete(ctx.handle, &status)
                        let str = String(data: Data(bytes: ffiStr.ptr!, count: Int(ffiStr.len)), encoding: .utf8)!
                        mffi_free_string(ffiStr)
                        mffi_async_greeting_free(ctx.handle)
                        ctx.continuation.resume(returning: str)
                    } else {
                        poll(ctx: ctx)
                    }
                }
            }
            poll(ctx: context)
        }
    } onCancel: {
        mffi_async_greeting_cancel(futureHandle)
        mffi_async_greeting_free(futureHandle)
    }
}


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


public final class DataStore {
    let handle: OpaquePointer

    private init(handle: OpaquePointer) {
        self.handle = handle
    }

    public convenience init() {
        let ptr = mffi_datastore_new()!
        self.init(handle: ptr)
    }

    deinit {
        _ = mffi_datastore_free(handle)
    }

    public func add(point: DataPoint) {
        mffi_datastore_add(handle, point)
    }

    public func len() -> UInt {
        return mffi_datastore_len(handle)
    }

    public func foreach(callback: @escaping (DataPoint) -> Void) {
        
        typealias ForeachCallbackFn = (DataPoint) -> Void
        class ForeachCallbackBox { let fn_: ForeachCallbackFn; init(_ fn_: @escaping ForeachCallbackFn) { self.fn_ = fn_ } }
        let callbackBox = ForeachCallbackBox(callback)
        let callbackPtr = Unmanaged.passRetained(callbackBox).toOpaque()
        let callbackTrampoline: @convention(c) (UnsafeMutableRawPointer?, DataPoint) -> Void = { ud, val in
            Unmanaged<ForeachCallbackBox>.fromOpaque(ud!).takeUnretainedValue().fn_(val)
        }
        let _ = mffi_datastore_foreach(handle, callbackTrampoline, callbackPtr)
        Unmanaged<ForeachCallbackBox>.fromOpaque(callbackPtr).release()
    }

    public func sum() -> Double {
        return mffi_datastore_sum(handle)
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
    guard let subscription = mffi_sensormonitor_readings(self.handle) else {
        continuation.finish()
        return
    }
    let buffer = UnsafeMutablePointer<SensorReading>.allocate(capacity: 16)
    
    continuation.onTermination = { @Sendable _ in
        mffi_sensormonitor_readings_unsubscribe(subscription)
        mffi_sensormonitor_readings_free(subscription)
        buffer.deallocate()
    }
    
    class StreamContext {
        let subscription: SubscriptionHandle
        let buffer: UnsafeMutablePointer<SensorReading>
        let continuation: AsyncStream<SensorReading>.Continuation
        var isActive = true
        
        init(subscription: SubscriptionHandle, buffer: UnsafeMutablePointer<SensorReading>, continuation: AsyncStream<SensorReading>.Continuation) {
            self.subscription = subscription
            self.buffer = buffer
            self.continuation = continuation
        }
    }
    
    let context = StreamContext(subscription: subscription, buffer: buffer, continuation: continuation)
    
    func poll(ctx: StreamContext) {
        guard ctx.isActive else { return }
        
        mffi_sensormonitor_readings_poll(ctx.subscription, UInt64(UInt(bitPattern: Unmanaged.passRetained(ctx).toOpaque()))) { callbackData, pollResult in
            let ctx = Unmanaged<StreamContext>.fromOpaque(UnsafeRawPointer(bitPattern: UInt(callbackData))!).takeRetainedValue()
            guard ctx.isActive else { return }
            
            let count = mffi_sensormonitor_readings_pop_batch(ctx.subscription, ctx.buffer, 16)
            if count > 0 {
                for i in 0..<Int(count) {
                    ctx.continuation.yield(ctx.buffer[i])
                }
            }
            
            if pollResult == 0 {
                poll(ctx: ctx)
            } else {
                ctx.isActive = false
                ctx.continuation.finish()
            }
        }
    }
    
    poll(ctx: context)
}
    }
}


public final class DataConsumer {
    let handle: OpaquePointer

    private init(handle: OpaquePointer) {
        self.handle = handle
    }

    public convenience init() {
        let ptr = mffi_dataconsumer_new()!
        self.init(handle: ptr)
    }

    deinit {
        _ = mffi_dataconsumer_free(handle)
    }

    public func setProvider(provider: DataProviderProtocol) {
        mffi_dataconsumer_set_provider(handle, UnsafeMutablePointer<ForeignDataProvider>(DataProviderBridge.create(provider)))
    }

    public func computeSum() -> UInt64 {
        return mffi_dataconsumer_compute_sum(handle)
    }
}


public protocol DataProviderProtocol: AnyObject {
    func getCount() -> UInt32
    func getItem(index: UInt32) -> DataPoint
}

private class DataProviderWrapper {
    let impl_: DataProviderProtocol
    init(_ impl_: DataProviderProtocol) { self.impl_ = impl_ }
}

private var dataProviderVTableInstance: DataProviderVTable = {
    DataProviderVTable(
        free: { handle in
            guard handle != 0 else { return }
            Unmanaged<DataProviderWrapper>.fromOpaque(UnsafeRawPointer(bitPattern: UInt(handle))!).release()
        },
        clone: { handle in
            guard handle != 0 else { return 0 }
            let wrapper = Unmanaged<DataProviderWrapper>.fromOpaque(UnsafeRawPointer(bitPattern: UInt(handle))!)
            _ = wrapper.retain()
            return handle
        },
        get_count: { handle, outPtr, statusPtr in
            guard handle != 0 else { statusPtr?.pointee = FfiStatus(code: 1); return }
            let wrapper = Unmanaged<DataProviderWrapper>.fromOpaque(UnsafeRawPointer(bitPattern: UInt(handle))!).takeUnretainedValue()
            let result = wrapper.impl_.getCount()
            outPtr?.pointee = result
            statusPtr?.pointee = FfiStatus(code: 0)
        },
        get_item: { handle, index, outPtr, statusPtr in
            guard handle != 0 else { statusPtr?.pointee = FfiStatus(code: 1); return }
            let wrapper = Unmanaged<DataProviderWrapper>.fromOpaque(UnsafeRawPointer(bitPattern: UInt(handle))!).takeUnretainedValue()
            let result = wrapper.impl_.getItem(index: index)
            outPtr?.pointee = result
            statusPtr?.pointee = FfiStatus(code: 0)
        }
    )
}()

public enum DataProviderBridge {
    private static var isRegistered = false
    
    public static func register() {
        guard !isRegistered else { return }
        mffi_register_data_provider_vtable(&dataProviderVTableInstance)
        isRegistered = true
    }
    
    public static func create(_ impl: DataProviderProtocol) -> OpaquePointer {
        register()
        let wrapper = DataProviderWrapper(impl)
        let handle = UInt64(UInt(bitPattern: Unmanaged.passRetained(wrapper).toOpaque()))
        return OpaquePointer(mffi_create_data_provider(handle)!)
    }
}


public protocol AsyncDataFetcherProtocol: AnyObject {
    func fetchValue(key: UInt32) async -> UInt64
}

private class AsyncDataFetcherWrapper {
    let impl_: AsyncDataFetcherProtocol
    init(_ impl_: AsyncDataFetcherProtocol) { self.impl_ = impl_ }
}

private var asyncDataFetcherVTableInstance: AsyncDataFetcherVTable = {
    AsyncDataFetcherVTable(
        free: { handle in
            guard handle != 0 else { return }
            Unmanaged<AsyncDataFetcherWrapper>.fromOpaque(UnsafeRawPointer(bitPattern: UInt(handle))!).release()
        },
        clone: { handle in
            guard handle != 0 else { return 0 }
            let wrapper = Unmanaged<AsyncDataFetcherWrapper>.fromOpaque(UnsafeRawPointer(bitPattern: UInt(handle))!)
            _ = wrapper.retain()
            return handle
        },
        fetch_value: { handle, key, callback, callbackData in
            guard handle != 0 else { return }
            let wrapper = Unmanaged<AsyncDataFetcherWrapper>.fromOpaque(UnsafeRawPointer(bitPattern: UInt(handle))!).takeUnretainedValue()
            Task {
                let result = await wrapper.impl_.fetchValue(key: key)
                callback?(callbackData, result, FfiStatus(code: 0))
            }
        }
    )
}()

public enum AsyncDataFetcherBridge {
    private static var isRegistered = false
    
    public static func register() {
        guard !isRegistered else { return }
        mffi_register_async_data_fetcher_vtable(&asyncDataFetcherVTableInstance)
        isRegistered = true
    }
    
    public static func create(_ impl: AsyncDataFetcherProtocol) -> OpaquePointer {
        register()
        let wrapper = AsyncDataFetcherWrapper(impl)
        let handle = UInt64(UInt(bitPattern: Unmanaged.passRetained(wrapper).toOpaque()))
        return OpaquePointer(mffi_create_async_data_fetcher(handle)!)
    }
}

