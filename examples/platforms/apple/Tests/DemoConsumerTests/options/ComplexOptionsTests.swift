import Demo
import XCTest

final class ComplexOptionsTests: XCTestCase {
    func testComplexOptionFns() {
        XCTAssertEqual(echoOptionalString(v: "hello"), "hello")
        XCTAssertNil(echoOptionalString(v: nil))
        XCTAssertEqual(isSomeString(v: "x"), true)
        XCTAssertEqual(isSomeString(v: nil), false)

        XCTAssertEqual(echoOptionalPoint(v: Point(x: 1.0, y: 2.0)), Point(x: 1.0, y: 2.0))
        XCTAssertNil(echoOptionalPoint(v: nil))
        XCTAssertEqual(makeSomePoint(x: 3.0, y: 4.0), Point(x: 3.0, y: 4.0))
        XCTAssertNil(makeNonePoint())

        XCTAssertEqual(echoOptionalStatus(v: .active), .active)
        XCTAssertNil(echoOptionalStatus(v: nil))
        XCTAssertEqual(echoOptionalVec(v: [1, 2, 3]), [1, 2, 3])
        XCTAssertNil(echoOptionalVec(v: nil))
        XCTAssertEqual(optionalVecLength(v: [9, 8]), 2)
        XCTAssertNil(optionalVecLength(v: nil))
    }
}

