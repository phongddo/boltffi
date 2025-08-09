import Foundation
import MobiFFI

print("Testing MobiFFI Swift binding...")

let major = mffi_version_major()
let minor = mffi_version_minor()
let patch = mffi_version_patch()

print("Version: \(major).\(minor).\(patch)")

let src: [UInt8] = [1, 2, 3, 4, 5]
var dst = [UInt8](repeating: 0, count: 10)
var written: UInt = 0

let srcCount = src.count
let dstCount = dst.count

let srcPtr = UnsafeMutablePointer<UInt8>.allocate(capacity: srcCount)
srcPtr.initialize(from: src, count: srcCount)

let status = dst.withUnsafeMutableBufferPointer { dstPtr in
    mffi_copy_bytes(srcPtr, UInt(srcCount), dstPtr.baseAddress, UInt(dstCount), &written)
}

srcPtr.deallocate()

print("copy_bytes status: \(status.code)")
print("written: \(written)")
print("dst: \(Array(dst.prefix(Int(written))))")

if status.code == 0 && written == 5 && Array(dst.prefix(5)) == src {
    print("SUCCESS: copy_bytes works!")
} else {
    print("FAILED: copy_bytes test failed")
    exit(1)
}

print("\n--- Testing opaque handles (Counter) ---")

let counter = mffi_counter_new(10)
print("Created counter with initial value 10")

var value: UInt64 = 0
var getStatus = mffi_counter_get(counter, &value)
print("Initial value: \(value), status: \(getStatus.code)")

let incStatus = mffi_counter_increment(counter)
print("Increment status: \(incStatus.code)")

getStatus = mffi_counter_get(counter, &value)
print("After increment: \(value), status: \(getStatus.code)")

let incStatus2 = mffi_counter_increment(counter)
let incStatus3 = mffi_counter_increment(counter)
getStatus = mffi_counter_get(counter, &value)
print("After 2 more increments: \(value)")

mffi_counter_free(counter)
print("Counter freed")

if value == 13 {
    print("SUCCESS: Opaque handles work correctly!")
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
var copied: UInt = 0

let copyStatus = points.withUnsafeMutableBufferPointer { ptr in
    mffi_datastore_copy_into(store, ptr.baseAddress, storeLen, &copied)
}

print("Copied \(copied) items, status: \(copyStatus.code)")

for (i, p) in points.enumerated() {
    print("  [\(i)] x=\(p.x), y=\(p.y), ts=\(p.timestamp)")
}

mffi_datastore_free(store)

if storeLen == 3 && copied == 3 && 
   points[0].x == 1.0 && points[1].x == 3.0 && points[2].x == 5.0 {
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

print("\n=== ALL TESTS PASSED ===")
