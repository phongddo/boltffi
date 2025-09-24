import Foundation
import MobiFFI

print("Testing MobiFFI Swift binding...")

let major = mffi_version_major()
let minor = mffi_version_minor()
let patch = mffi_version_patch()

print("Version: \(major).\(minor).\(patch)")

let src: [UInt8] = [1, 2, 3, 4, 5]
var dst = [UInt8](repeating: 0, count: 10)

let srcCount = src.count
let dstCount = dst.count

let srcPtr = UnsafeMutablePointer<UInt8>.allocate(capacity: srcCount)
srcPtr.initialize(from: src, count: srcCount)

let written = dst.withUnsafeMutableBufferPointer { dstPtr in
    mffi_copy_bytes(srcPtr, UInt(srcCount), dstPtr.baseAddress, UInt(dstCount))
}

srcPtr.deallocate()

print("written: \(written)")
print("dst: \(Array(dst.prefix(Int(written))))")

if written == 5 && Array(dst.prefix(5)) == src {
    print("SUCCESS: copy_bytes works!")
} else {
    print("FAILED: copy_bytes test failed")
    exit(1)
}

print("\n--- Testing opaque handles (Counter via ffi_class macro) ---")

let counter = mffi_counter_new()!
mffi_counter_set(counter, 10)
print("Created counter and set initial value to 10")

var value = mffi_counter_get(counter)
print("Initial value: \(value)")

mffi_counter_increment(counter)
print("Incremented")

value = mffi_counter_get(counter)
print("After increment: \(value)")

mffi_counter_increment(counter)
mffi_counter_increment(counter)
value = mffi_counter_get(counter)
print("After 2 more increments: \(value)")

mffi_counter_free(counter)
print("Counter freed")

if value == 13 {
    print("SUCCESS: ffi_class Counter works correctly!")
} else {
    print("FAILED: Expected 13, got \(value)")
    exit(1)
}

print("\n--- Testing Vec bulk copy (DataStore) ---")

let store = mffi_datastore_new()

var p1 = DataPoint(x: 1.0, y: 2.0, timestamp: 100)
var p2 = DataPoint(x: 3.0, y: 4.0, timestamp: 200)
var p3 = DataPoint(x: 5.0, y: 6.0, timestamp: 300)

_ = mffi_datastore_add(store, p1)
_ = mffi_datastore_add(store, p2)
_ = mffi_datastore_add(store, p3)

let storeLen = mffi_datastore_len(store)
print("DataStore has \(storeLen) items")

var points = [DataPoint](repeating: DataPoint(x: 0, y: 0, timestamp: 0), count: Int(storeLen))

let copied = points.withUnsafeMutableBufferPointer { ptr in
    mffi_datastore_copy_into(store, ptr.baseAddress, storeLen)
}

print("Copied \(copied) items")

for (i, p) in points.enumerated() {
    print("  [\(i)] x=\(p.x), y=\(p.y), ts=\(p.timestamp)")
}

mffi_datastore_free(store)

if storeLen == 3 && copied == 3 && points[0].x == 1.0 && points[1].x == 3.0 && points[2].x == 5.0 {
    print("SUCCESS: Vec bulk copy works!")
} else {
    print("FAILED: Vec bulk copy test failed")
    exit(1)
}

print("\n--- Testing FfiString returns ---")

func testGreeting() -> Bool {
    let name = "Ali"
    var result = FfiString(ptr: nil, len: 0, cap: 0)

    let greetStatus = name.withCString { namePtr in
        mffi_greeting(namePtr, UInt(name.utf8.count), &result)
    }

    guard greetStatus.code == 0 else {
        print("FAILED: greeting returned status \(greetStatus.code)")
        return false
    }

    let data = Data(bytes: result.ptr, count: Int(result.len))
    let greeting = String(data: data, encoding: .utf8)

    print("Greeting: \(greeting ?? "nil")")

    mffi_free_string(result)
    print("String freed")

    return greeting == "Hello, Ali!"
}

func testConcat() -> Bool {
    let first = "Mobi"
    let second = "FFI"
    var result = FfiString(ptr: nil, len: 0, cap: 0)

    let concatStatus = first.withCString { firstPtr in
        second.withCString { secondPtr in
            mffi_concat(
                firstPtr, UInt(first.utf8.count),
                secondPtr, UInt(second.utf8.count),
                &result
            )
        }
    }

    guard concatStatus.code == 0 else {
        print("FAILED: concat returned status \(concatStatus.code)")
        return false
    }

    let data = Data(bytes: result.ptr, count: Int(result.len))
    let concatenated = String(data: data, encoding: .utf8)

    print("Concatenated: \(concatenated ?? "nil")")

    mffi_free_string(result)

    return concatenated == "MobiFFI"
}

if testGreeting() {
    print("SUCCESS: Greeting works!")
} else {
    print("FAILED: Greeting test failed")
    exit(1)
}

if testConcat() {
    print("SUCCESS: Concat works!")
} else {
    print("FAILED: Concat test failed")
    exit(1)
}

func testReverseString() -> Bool {
    let input = "Hello"
    var result = FfiString(ptr: nil, len: 0, cap: 0)

    let status = input.withCString { inputPtr in
        mffi_reverse_string(inputPtr, UInt(input.utf8.count), &result)
    }

    guard status.code == 0 else {
        print("FAILED: reverse_string returned status \(status.code)")
        return false
    }

    let data = Data(bytes: result.ptr, count: Int(result.len))
    let reversed = String(data: data, encoding: .utf8)

    print("Reversed: \(reversed ?? "nil")")

    mffi_free_string(result)

    return reversed == "olleH"
}

if testReverseString() {
    print("SUCCESS: String param works!")
} else {
    print("FAILED: reverse_string test failed")
    exit(1)
}

print("\n--- Testing error messages ---")

func testErrorMessage() -> Bool {
    var result = FfiString(ptr: nil, len: 0, cap: 0)

    let invalidUtf8: [UInt8] = [0xFF, 0xFE, 0x00]
    let status = invalidUtf8.withUnsafeBufferPointer { ptr in
        mffi_greeting(ptr.baseAddress, UInt(invalidUtf8.count), &result)
    }

    guard status.code != 0 else {
        print("FAILED: Expected error for invalid UTF-8")
        return false
    }

    print("Got expected error, status code: \(status.code)")

    var errorMsg = FfiString(ptr: nil, len: 0, cap: 0)
    let msgStatus = mffi_last_error_message(&errorMsg)

    guard msgStatus.code == 0 else {
        print("FAILED: Could not get error message")
        return false
    }

    let data = Data(bytes: errorMsg.ptr, count: Int(errorMsg.len))
    let message = String(data: data, encoding: .utf8) ?? ""

    print("Error message: \(message)")

    mffi_free_string(errorMsg)

    return message.contains("UTF-8")
}

if testErrorMessage() {
    print("SUCCESS: Error messages work!")
} else {
    print("FAILED: Error message test failed")
    exit(1)
}

print("\n--- Testing callbacks ---")

class CallbackContext {
    var points: [DataPoint] = []
    var sumResult: Double = 0
}

func testForEachCallback() -> Bool {
    let store = mffi_datastore_new()

    _ = mffi_datastore_add(store, DataPoint(x: 1.0, y: 2.0, timestamp: 100))
    _ = mffi_datastore_add(store, DataPoint(x: 3.0, y: 4.0, timestamp: 200))
    _ = mffi_datastore_add(store, DataPoint(x: 5.0, y: 6.0, timestamp: 300))

    let context = CallbackContext()
    let contextPtr = Unmanaged.passUnretained(context).toOpaque()

    let callback: @convention(c) (UnsafeMutableRawPointer?, DataPoint) -> Void = {
        userData, point in
        guard let ptr = userData else { return }
        let ctx = Unmanaged<CallbackContext>.fromOpaque(ptr).takeUnretainedValue()
        ctx.points.append(point)
    }

    let status = mffi_datastore_foreach(store, callback, contextPtr)
    mffi_datastore_free(store)

    guard status.code == 0 else {
        print("FAILED: foreach returned status \(status.code)")
        return false
    }

    print("forEach collected \(context.points.count) points:")
    for (i, p) in context.points.enumerated() {
        print("  [\(i)] x=\(p.x), y=\(p.y)")
    }

    return context.points.count == 3 && context.points[0].x == 1.0 && context.points[2].x == 5.0
}

func testSum() -> Bool {
    let store = mffi_datastore_new()

    _ = mffi_datastore_add(store, DataPoint(x: 1.0, y: 2.0, timestamp: 100))
    _ = mffi_datastore_add(store, DataPoint(x: 3.0, y: 4.0, timestamp: 200))

    let sum = mffi_datastore_sum(store)
    mffi_datastore_free(store)

    print("Sum result: \(sum)")

    return sum == 10.0
}

if testForEachCallback() {
    print("SUCCESS: forEach callback works!")
} else {
    print("FAILED: forEach callback test failed")
    exit(1)
}

if testSum() {
    print("SUCCESS: Sum callback works!")
} else {
    print("FAILED: Sum callback test failed")
    exit(1)
}

print("\n--- Testing macro-generated exports ---")

let addResult = mffi_add_numbers(7, 8)
print("mffi_add_numbers(7, 8) = \(addResult)")

let mulResult = mffi_multiply_floats(3.5, 2.0)
print("mffi_multiply_floats(3.5, 2.0) = \(mulResult)")

if addResult == 15 && mulResult == 7.0 {
    print("SUCCESS: Basic macro exports work!")
} else {
    print("FAILED: Macro test failed")
    exit(1)
}

print("\n--- Testing macro String return ---")

var greetingOut = FfiString()
let name = "Rust"
let greetingStatus = name.withCString { ptr in
    mffi_make_greeting(ptr, UInt(name.utf8.count), &greetingOut)
}

if greetingStatus.code == 0 {
    let greetingStr = String(cString: greetingOut.ptr)
    print("Greeting: \(greetingStr)")
    mffi_free_string(greetingOut)

    if greetingStr == "Hello, Rust!" {
        print("SUCCESS: Macro String return works!")
    } else {
        print("FAILED: Wrong greeting '\(greetingStr)'")
        exit(1)
    }
} else {
    print("FAILED: make_greeting returned error")
    exit(1)
}

print("\n--- Testing macro Result return ---")

var divResult: Int32 = 0
let divStatus = mffi_safe_divide(10, 3, &divResult)
print("mffi_safe_divide(10, 3) = \(divResult), status = \(divStatus.code)")

if divStatus.code == 0 && divResult == 3 {
    print("SUCCESS: Result<i32> OK path works!")
} else {
    print("FAILED: Expected 3, got \(divResult)")
    exit(1)
}

var divByZeroResult: Int32 = 0
let divByZeroStatus = mffi_safe_divide(10, 0, &divByZeroResult)
print("mffi_safe_divide(10, 0) status = \(divByZeroStatus.code)")

if divByZeroStatus.code != 0 {
    var errOut = FfiString()
    let errStatus = mffi_last_error_message(&errOut)
    if errStatus.code == 0 && errOut.ptr != nil {
        let errStr = String(cString: errOut.ptr)
        print("Error message: \(errStr)")
        mffi_free_string(errOut)
        if errStr.contains("division by zero") {
            print("SUCCESS: Result<i32> Err path works!")
        } else {
            print("FAILED: Wrong error message '\(errStr)'")
            exit(1)
        }
    } else {
        print("WARNING: No error message but status indicated error")
    }
} else {
    print("FAILED: Division by zero should have failed")
    exit(1)
}

print("\n--- Testing macro Vec return ---")

let seqLen = mffi_generate_sequence_len(5)
print("mffi_generate_sequence_len(5) = \(seqLen)")

if seqLen == 5 {
    var seqBuffer = [Int32](repeating: 0, count: Int(seqLen))
    var written: UInt = 0
    let seqStatus = seqBuffer.withUnsafeMutableBufferPointer { ptr in
        mffi_generate_sequence_copy_into(5, ptr.baseAddress!, seqLen, &written)
    }

    print("Sequence: \(seqBuffer), written: \(written)")

    if seqStatus.code == 0 && written == 5 && seqBuffer == [0, 1, 2, 3, 4] {
        print("SUCCESS: Vec<i32> return works!")
    } else {
        print("FAILED: Vec test failed")
        exit(1)
    }
} else {
    print("FAILED: Expected len 5, got \(seqLen)")
    exit(1)
}

print("\n--- Testing callback macro ---")

class SumCollector {
    var sum: Int32 = 0
}

let collector = SumCollector()
let collectorPtr = Unmanaged.passUnretained(collector).toOpaque()

let foreachCallback: @convention(c) (UnsafeMutableRawPointer?, Int32) -> Void = { userData, value in
    let collector = Unmanaged<SumCollector>.fromOpaque(userData!).takeUnretainedValue()
    collector.sum += value
}

let foreachStatus = mffi_foreach_range(0, 5, foreachCallback, collectorPtr)
print("foreach_range(0, 5) sum = \(collector.sum), status = \(foreachStatus.code)")

if collector.sum == 10 && foreachStatus.code == 0 {
    print("SUCCESS: Callback macro works!")
} else {
    print("FAILED: Expected sum 10, got \(collector.sum)")
    exit(1)
}

print("\n--- Testing ffi_class macro ---")

let acc = mffi_accumulator_new()!
print("Created Accumulator")

mffi_accumulator_add(acc, 10)
mffi_accumulator_add(acc, 5)
mffi_accumulator_add(acc, 7)

let accValue = mffi_accumulator_get(acc)
print("Accumulator value after adds: \(accValue)")

if accValue == 22 {
    print("SUCCESS: ffi_class methods work!")
} else {
    print("FAILED: Expected 22, got \(accValue)")
    mffi_accumulator_free(acc)
    exit(1)
}

mffi_accumulator_reset(acc)
let resetValue = mffi_accumulator_get(acc)
print("Accumulator value after reset: \(resetValue)")

if resetValue == 0 {
    print("SUCCESS: ffi_class reset works!")
} else {
    print("FAILED: Expected 0 after reset, got \(resetValue)")
    mffi_accumulator_free(acc)
    exit(1)
}

mffi_accumulator_free(acc)
print("Freed Accumulator")

print("\n--- Testing C-style enums ---")

let north = Direction_North
let opposite = mffi_opposite_direction(north)
print("opposite_direction(North) = \(opposite)")

if opposite == Direction_South {
    print("SUCCESS: opposite_direction works!")
} else {
    print("FAILED: Expected South(\(Direction_South)), got \(opposite)")
    exit(1)
}

let degrees = mffi_direction_to_degrees(Direction_East)
print("direction_to_degrees(East) = \(degrees)")

if degrees == 90 {
    print("SUCCESS: direction_to_degrees works!")
} else {
    print("FAILED: Expected 90, got \(degrees)")
    exit(1)
}

print("\n--- Testing Option returns ---")

var evenResult: Int32 = 0
let hasSome = mffi_find_even(4, &evenResult)
print("find_even(4) = \(hasSome), value = \(evenResult)")

if hasSome == 1 && evenResult == 4 {
    print("SUCCESS: Option Some works!")
} else {
    print("FAILED: Expected Some(4), got hasSome=\(hasSome), value=\(evenResult)")
    exit(1)
}

var noneResult: Int32 = 0
let hasNone = mffi_find_even(3, &noneResult)
print("find_even(3) = \(hasNone)")

if hasNone == 0 {
    print("SUCCESS: Option None works!")
} else {
    print("FAILED: Expected None (0), got \(hasNone)")
    exit(1)
}

print("\n--- Testing data enums (tagged unions) ---")

let successResult = mffi_process_value(5)
print("process_value(5): tag=\(successResult.tag)")
if successResult.tag == ApiResult_TAG_Success {
    print("SUCCESS: ApiResult::Success works!")
} else {
    print("FAILED: Expected Success tag (0), got \(successResult.tag)")
    exit(1)
}

let errorCodeResult = mffi_process_value(0)
print("process_value(0): tag=\(errorCodeResult.tag), ErrorCode=\(errorCodeResult.payload.ErrorCode)")
if errorCodeResult.tag == ApiResult_TAG_ErrorCode && errorCodeResult.payload.ErrorCode == -1 {
    print("SUCCESS: ApiResult::ErrorCode works!")
} else {
    print("FAILED: Expected ErrorCode tag with -1")
    exit(1)
}

let errorWithDataResult = mffi_process_value(-3)
print("process_value(-3): tag=\(errorWithDataResult.tag), code=\(errorWithDataResult.payload.ErrorWithData.code), detail=\(errorWithDataResult.payload.ErrorWithData.detail)")
if errorWithDataResult.tag == ApiResult_TAG_ErrorWithData && 
   errorWithDataResult.payload.ErrorWithData.code == -3 && 
   errorWithDataResult.payload.ErrorWithData.detail == -6 {
    print("SUCCESS: ApiResult::ErrorWithData works!")
} else {
    print("FAILED: Expected ErrorWithData with code=-3, detail=-6")
    exit(1)
}

let isSuccess = mffi_api_result_is_success(successResult)
print("api_result_is_success(Success) = \(isSuccess)")
if isSuccess {
    print("SUCCESS: api_result_is_success works!")
} else {
    print("FAILED: Expected true for Success variant")
    exit(1)
}

print("\n--- Testing poll-based async (foreign-driven) ---")

import Dispatch

class PollContext {
    var pollResult: Int8 = -1
    let semaphore = DispatchSemaphore(value: 0)
}

let pollContinuation: @convention(c) (UInt64, Int8) -> Void = { callbackData, pollResult in
    let ptr = UnsafeMutableRawPointer(bitPattern: UInt(callbackData))!
    let ctx = Unmanaged<PollContext>.fromOpaque(ptr).takeUnretainedValue()
    ctx.pollResult = pollResult
    ctx.semaphore.signal()
}

func pollUntilReady<T>(
    handle: RustFutureHandle?,
    poll: (RustFutureHandle?, UInt64, RustFutureContinuationCallback?) -> Void,
    complete: (RustFutureHandle?, UnsafeMutablePointer<FfiStatus>?) -> T,
    free: (RustFutureHandle?) -> Void
) -> (FfiStatus, T)? {
    guard let handle = handle else { return nil }
    let pollCtx = PollContext()
    let pollCtxPtr = Unmanaged.passRetained(pollCtx).toOpaque()
    let callbackData = UInt64(UInt(bitPattern: pollCtxPtr))
    
    var maxPolls = 10
    while maxPolls > 0 {
        poll(handle, callbackData, pollContinuation)
        let waitResult = pollCtx.semaphore.wait(timeout: .now() + 5.0)
        if waitResult == .timedOut {
            Unmanaged<PollContext>.fromOpaque(pollCtxPtr).release()
            free(handle)
            return nil
        }
        
        if pollCtx.pollResult == 0 {
            var status = FfiStatus()
            let result = complete(handle, &status)
            free(handle)
            Unmanaged<PollContext>.fromOpaque(pollCtxPtr).release()
            return (status, result)
        }
        maxPolls -= 1
    }
    Unmanaged<PollContext>.fromOpaque(pollCtxPtr).release()
    free(handle)
    return nil
}

let computeHandle = mffi_compute_heavy(21)
print("Created async future handle")

if let (status, result) = pollUntilReady(
    handle: computeHandle,
    poll: mffi_compute_heavy_poll,
    complete: mffi_compute_heavy_complete,
    free: mffi_compute_heavy_free
) {
    print("Async result: status=\(status.code), result=\(result)")
    if status.code == 0 && result == 42 {
        print("SUCCESS: Poll-based async i32 works! (21 * 2 = 42)")
    } else {
        print("FAILED: Expected status=0, result=42")
        exit(1)
    }
} else {
    print("FAILED: Async operation timed out or failed")
    exit(1)
}

print("\n--- Testing async with Result return ---")

let fetchHandle = mffi_fetch_data(5)
if let (status, result) = pollUntilReady(
    handle: fetchHandle,
    poll: mffi_fetch_data_poll,
    complete: mffi_fetch_data_complete,
    free: mffi_fetch_data_free
) {
    print("Fetch result: status=\(status.code), result=\(result)")
    if status.code == 0 && result == 50 {
        print("SUCCESS: Async Result Ok works! (5 * 10 = 50)")
    } else {
        print("FAILED: Expected status=0, result=50")
        exit(1)
    }
} else {
    print("FAILED: Async fetch timed out")
    exit(1)
}

print("\n--- Testing async String return ---")

let strHandle = mffi_async_make_string(42)
if let (status, result) = pollUntilReady(
    handle: strHandle,
    poll: mffi_async_make_string_poll,
    complete: mffi_async_make_string_complete,
    free: mffi_async_make_string_free
) {
    let strResult = result.ptr != nil ? String(cString: result.ptr!) : ""
    print("async_make_string result: status=\(status.code), value=\"\(strResult)\"")
    mffi_free_string(result)
    if status.code == 0 && strResult == "Value is: 42" {
        print("SUCCESS: Async String return works!")
    } else {
        print("FAILED: Expected 'Value is: 42', got '\(strResult)'")
        exit(1)
    }
} else {
    print("FAILED: Async String timed out")
    exit(1)
}

print("\n--- Testing async struct return ---")

let pointHandle = mffi_async_fetch_point(3.14, 2.71)
if let (status, result) = pollUntilReady(
    handle: pointHandle,
    poll: mffi_async_fetch_point_poll,
    complete: mffi_async_fetch_point_complete,
    free: mffi_async_fetch_point_free
) {
    print("async_fetch_point result: status=\(status.code), x=\(result.x), y=\(result.y)")
    if status.code == 0 && result.x == 3.14 && result.y == 2.71 {
        print("SUCCESS: Async struct return works!")
    } else {
        print("FAILED: Expected x=3.14, y=2.71")
        exit(1)
    }
} else {
    print("FAILED: Async struct timed out")
    exit(1)
}

print("\n--- Testing async Vec return ---")

let vecHandle = mffi_async_get_numbers(5)
if let (status, result) = pollUntilReady(
    handle: vecHandle,
    poll: mffi_async_get_numbers_poll,
    complete: mffi_async_get_numbers_complete,
    free: mffi_async_get_numbers_free
) {
    print("async_get_numbers result: status=\(status.code), len=\(result.len)")
    if status.code == 0 && result.len == 5 {
        print("SUCCESS: Async Vec return works!")
    } else {
        print("FAILED: Expected len=5")
        exit(1)
    }
    mffi_free_buf_i32(result)
} else {
    print("FAILED: Async Vec timed out")
    exit(1)
}

print("\n--- Testing async Option return ---")

let optHandle = mffi_async_find_value(5)
if let (status, result) = pollUntilReady(
    handle: optHandle,
    poll: mffi_async_find_value_poll,
    complete: mffi_async_find_value_complete,
    free: mffi_async_find_value_free
) {
    print("async_find_value(5) result: status=\(status.code), isSome=\(result.isSome), value=\(result.value)")
    if status.code == 0 && result.isSome && result.value == 500 {
        print("SUCCESS: Async Option Some works!")
    } else {
        print("FAILED: Expected Some(500)")
        exit(1)
    }
} else {
    print("FAILED: Async Option timed out")
    exit(1)
}

let optNoneHandle = mffi_async_find_value(-1)
if let (status, result) = pollUntilReady(
    handle: optNoneHandle,
    poll: mffi_async_find_value_poll,
    complete: mffi_async_find_value_complete,
    free: mffi_async_find_value_free
) {
    print("async_find_value(-1) result: status=\(status.code), isSome=\(result.isSome)")
    if status.code == 0 && !result.isSome {
        print("SUCCESS: Async Option None works!")
    } else {
        print("FAILED: Expected None")
        exit(1)
    }
} else {
    print("FAILED: Async Option None timed out")
    exit(1)
}

print("\n--- Testing async with &str param ---")

let asyncName = "World"
let asyncNameData = Array(asyncName.utf8)
let greetHandle = asyncNameData.withUnsafeBufferPointer { buf in
    mffi_async_greeting(buf.baseAddress, UInt(buf.count))
}
if let (status, result) = pollUntilReady(
    handle: greetHandle,
    poll: mffi_async_greeting_poll,
    complete: mffi_async_greeting_complete,
    free: mffi_async_greeting_free
) {
    let greetResult = result.ptr != nil ? String(cString: result.ptr!) : ""
    print("async_greeting result: status=\(status.code), value=\"\(greetResult)\"")
    mffi_free_string(result)
    if status.code == 0 && greetResult == "Hello, World!" {
        print("SUCCESS: Async &str param works!")
    } else {
        print("FAILED: Expected 'Hello, World!', got '\(greetResult)'")
        exit(1)
    }
} else {
    print("FAILED: Async greeting timed out")
    exit(1)
}

print("\n--- Testing async Result<Vec<T>, E> ---")

let resultVecHandle = mffi_async_fetch_numbers(4)
if let (status, result) = pollUntilReady(
    handle: resultVecHandle,
    poll: mffi_async_fetch_numbers_poll,
    complete: mffi_async_fetch_numbers_complete,
    free: mffi_async_fetch_numbers_free
) {
    print("async_fetch_numbers(4) result: status=\(status.code), len=\(result.len)")
    if status.code == 0 && result.len == 4 {
        print("SUCCESS: Async Result<Vec<T>, E> Ok works!")
    } else {
        print("FAILED: Expected len=4")
        exit(1)
    }
    mffi_free_buf_i32(result)
} else {
    print("FAILED: Async Result<Vec> timed out")
    exit(1)
}

print("\n--- Testing streaming with ring buffer ---")

let subscription = mffi_test_events_subscribe(64)
print("Created subscription with capacity 64")

var pushSuccessCount = 0
for eventIndex in Int32(0)..<Int32(10) {
    if mffi_test_events_push(subscription, eventIndex, Int64(eventIndex * 100)) {
        pushSuccessCount += 1
    }
}
print("Pushed \(pushSuccessCount) events")

if pushSuccessCount != 10 {
    print("FAILED: Expected to push 10 events")
    mffi_test_events_free(subscription)
    exit(1)
}

var eventBuffer = [TestEvent](repeating: TestEvent(eventId: 0, value: 0), count: 4)
let batchCount1 = eventBuffer.withUnsafeMutableBufferPointer { buffer in
    mffi_test_events_pop_batch(subscription, buffer.baseAddress, UInt(buffer.count))
}
print("Popped first batch: \(batchCount1) events")

if batchCount1 != 4 {
    print("FAILED: Expected to pop 4 events, got \(batchCount1)")
    mffi_test_events_free(subscription)
    exit(1)
}

var allCorrect = true
for index in 0..<Int(batchCount1) {
    let expected_id = Int32(index)
    let expected_value = Int64(index * 100)
    if eventBuffer[index].eventId != expected_id || eventBuffer[index].value != expected_value {
        print("FAILED: Event \(index) mismatch: got id=\(eventBuffer[index].eventId), value=\(eventBuffer[index].value)")
        allCorrect = false
    }
}

if allCorrect {
    print("SUCCESS: Ring buffer batch pop works with correct FIFO order!")
} else {
    mffi_test_events_free(subscription)
    exit(1)
}

let batchCount2 = eventBuffer.withUnsafeMutableBufferPointer { buffer in
    mffi_test_events_pop_batch(subscription, buffer.baseAddress, UInt(buffer.count))
}
print("Popped second batch: \(batchCount2) events")

let batchCount3 = eventBuffer.withUnsafeMutableBufferPointer { buffer in
    mffi_test_events_pop_batch(subscription, buffer.baseAddress, UInt(buffer.count))
}
print("Popped third batch: \(batchCount3) events (drained all)")

let waitResultTimeout = mffi_test_events_wait(subscription, 10)
print("Wait with empty buffer (10ms timeout): \(waitResultTimeout) (0=Timeout)")

if waitResultTimeout != 0 {
    print("FAILED: Expected timeout (0), got \(waitResultTimeout)")
    mffi_test_events_free(subscription)
    exit(1)
}
print("SUCCESS: Wait timeout works!")

mffi_test_events_push(subscription, 999, 9999)
let waitResultAvailable = mffi_test_events_wait(subscription, 1000)
print("Wait with events available: \(waitResultAvailable) (1=EventsAvailable)")

if waitResultAvailable != 1 {
    print("FAILED: Expected events available (1), got \(waitResultAvailable)")
    mffi_test_events_free(subscription)
    exit(1)
}
print("SUCCESS: Wait with events works!")

mffi_test_events_unsubscribe(subscription)
let pushAfterUnsubscribe = mffi_test_events_push(subscription, 1000, 10000)
print("Push after unsubscribe: \(pushAfterUnsubscribe) (should be false)")

if pushAfterUnsubscribe {
    print("FAILED: Push should fail after unsubscribe")
    mffi_test_events_free(subscription)
    exit(1)
}
print("SUCCESS: Unsubscribe stops new pushes!")

let waitAfterUnsubscribe = mffi_test_events_wait(subscription, 100)
print("Wait after unsubscribe: \(waitAfterUnsubscribe) (-1=Unsubscribed)")

if waitAfterUnsubscribe != -1 {
    print("FAILED: Expected unsubscribed (-1), got \(waitAfterUnsubscribe)")
    mffi_test_events_free(subscription)
    exit(1)
}
print("SUCCESS: Wait returns unsubscribed after unsubscribe!")

mffi_test_events_free(subscription)
print("Freed subscription")

print("\n=== ALL TESTS PASSED ===")
