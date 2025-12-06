use std::path::Path;
use riff_verify::{Verifier, Reporter, VerificationResult};

#[test]
fn test_verify_generated_benchriff() {
    let swift_path = Path::new("../bench_demo/rust-riff/dist/BenchRiff.swift");
    
    if !swift_path.exists() {
        eprintln!("Skipping test: BenchRiff.swift not found (run `riff pack` first)");
        return;
    }

    let mut verifier = Verifier::swift().expect("failed to create verifier");
    let result = verifier.verify_file(swift_path).expect("failed to verify");
    
    let reporter = Reporter::human();
    eprintln!("{}", reporter.report(&result));
    
    eprintln!("Verified {} functions", match &result {
        VerificationResult::Verified { unit_count, .. } => unit_count,
        VerificationResult::Failed { .. } => &0,
    });
}

#[test]
fn test_verify_simple_generated_patterns() {
    let source = r#"
import Foundation

public struct FfiString {
    var ptr: UnsafePointer<UInt8>?
    var len: UInt
    var cap: UInt
}

public struct FfiStatus {
    var code: Int32
}

private func stringFromFfi(_ ffi: FfiString) -> String {
    guard let ptr = ffi.ptr else { return "" }
    return String(cString: ptr)
}

private func ensureOk(_ status: FfiStatus) {
    if status.code != 0 {
        fatalError("FFI error: \(status.code)")
    }
}

public func generateLocations(count: Int32) -> [Location] {
    let len = riff_generate_locations_len(count)
    let ptr = UnsafeMutablePointer<Location>.allocate(capacity: Int(len))
    defer { ptr.deallocate() }
    var written: UInt = 0
    let status = riff_generate_locations_copy_into(count, ptr, len, &written)
    ensureOk(status)
    return Array(UnsafeBufferPointer(start: ptr, count: Int(written)))
}

public func echoString(value: String) -> String {
    var result = FfiString(ptr: nil, len: 0, cap: 0)
    return value.withCString { valuePtr in
        let status = riff_echo_string(UnsafeRawPointer(valuePtr).assumingMemoryBound(to: UInt8.self), UInt(value.utf8.count), &result)
        defer { riff_free_string(result) }
        ensureOk(status)
        return stringFromFfi(result)
    }
}
"#;

    let mut verifier = Verifier::swift().expect("failed to create verifier");
    let result = verifier.verify_source(std::path::Path::new("test.swift"), source).expect("failed to verify");
    
    let reporter = Reporter::human();
    eprintln!("{}", reporter.report(&result));
}
