import Demo
import XCTest

final class UnsafeSingleThreadedTests: XCTestCase {
    func testStateHolderSyncAndAsyncMethods() async throws {
        let stateHolder = StateHolder(label: "local")
        XCTAssertEqual(stateHolder.getLabel(), "local")
        XCTAssertEqual(stateHolder.getValue(), 0)
        stateHolder.setValue(value: 5)
        XCTAssertEqual(stateHolder.getValue(), 5)
        XCTAssertEqual(stateHolder.increment(), 6)
        stateHolder.addItem(item: "a")
        stateHolder.addItem(item: "b")
        XCTAssertEqual(stateHolder.itemCount(), 2)
        XCTAssertEqual(stateHolder.getItems(), ["a", "b"])
        XCTAssertEqual(stateHolder.removeLast(), "b")
        XCTAssertEqual(stateHolder.transformValue(f: { $0 / 2 }), 3)
        let asyncValue = try await stateHolder.asyncGetValue()
        XCTAssertEqual(asyncValue, 3)
        try await stateHolder.asyncSetValue(value: 9)
        XCTAssertEqual(stateHolder.getValue(), 9)
        let asyncItemCount = try await stateHolder.asyncAddItem(item: "z")
        XCTAssertEqual(asyncItemCount, 2)
        XCTAssertEqual(stateHolder.getItems(), ["a", "z"])
        stateHolder.clear()
        XCTAssertEqual(stateHolder.getValue(), 0)
        XCTAssertEqual(stateHolder.getItems(), [])
    }
}

