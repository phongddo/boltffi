import Demo
import XCTest

final class ThreadSafeTests: XCTestCase {
    func testSharedCounterSyncAndAsyncMethods() async throws {
        let sharedCounter = SharedCounter(initial: 5)
        XCTAssertEqual(sharedCounter.get(), 5)
        sharedCounter.set(value: 6)
        XCTAssertEqual(sharedCounter.get(), 6)
        XCTAssertEqual(sharedCounter.increment(), 7)
        XCTAssertEqual(sharedCounter.add(amount: 3), 10)
        let asyncValue = try await sharedCounter.asyncGet()
        XCTAssertEqual(asyncValue, 10)
        let asyncAddedValue = try await sharedCounter.asyncAdd(amount: 5)
        XCTAssertEqual(asyncAddedValue, 15)
        XCTAssertEqual(sharedCounter.get(), 15)
    }
}

