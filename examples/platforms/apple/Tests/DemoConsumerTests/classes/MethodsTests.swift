import Demo
import XCTest

final class MethodsTests: XCTestCase {
    func testCounterValueAndErrorMethods() throws {
        let counter = Counter(initial: 2)
        XCTAssertEqual(counter.get(), 2)
        counter.increment()
        XCTAssertEqual(counter.get(), 3)
        counter.add(amount: 7)
        XCTAssertEqual(counter.get(), 10)
        XCTAssertEqual(try counter.tryGetPositive(), 10)
        XCTAssertEqual(counter.maybeDouble(), 20)
        XCTAssertEqual(counter.asPoint(), Point(x: 10.0, y: 0.0))
        counter.reset()
        XCTAssertEqual(counter.get(), 0)
        XCTAssertNil(counter.maybeDouble())
        assertThrowsMessageContains("count is not positive", try counter.tryGetPositive())
    }
}

