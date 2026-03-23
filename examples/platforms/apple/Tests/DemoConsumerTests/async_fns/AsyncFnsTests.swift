import Demo
import XCTest

final class AsyncFnsTests: XCTestCase {
    func testAsyncFns() async throws {
        let sum = try await asyncAdd(a: 3, b: 7)
        XCTAssertEqual(sum, 10)
        let echoedMessage = try await asyncEcho(message: "hello async")
        XCTAssertEqual(echoedMessage, "Echo: hello async")
        let doubledValues = try await asyncDoubleAll(values: [1, 2, 3])
        XCTAssertEqual(doubledValues, [2, 4, 6])
        let firstPositive = try await asyncFindPositive(values: [-1, 0, 5, 3])
        XCTAssertEqual(firstPositive, 5)
        let missingPositive = try await asyncFindPositive(values: [-1, -2, -3])
        XCTAssertNil(missingPositive)
        let concatenated = try await asyncConcat(strings: ["a", "b", "c"])
        XCTAssertEqual(concatenated, "a, b, c")
    }
}
