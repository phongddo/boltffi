import Demo
import XCTest

final class CStyleEnumsTests: XCTestCase {
    func testStatusFns() {
        XCTAssertEqual(echoStatus(s: .active), .active)
        XCTAssertEqual(statusToString(s: .active), "active")
        XCTAssertEqual(isActive(s: .pending), false)
        XCTAssertEqual(echoVecStatus(values: [.active, .pending]), [.active, .pending])
    }

    func testDirectionFns() {
        XCTAssertEqual(Direction(raw: 3), .west)
        XCTAssertEqual(Direction.cardinal(), .north)
        XCTAssertEqual(Direction(fromDegrees: 90.0), .east)
        XCTAssertEqual(Direction.count(), 4)
        XCTAssertEqual(Direction.north.opposite(), .south)
        XCTAssertEqual(Direction.east.isHorizontal(), true)
        XCTAssertEqual(Direction.west.label(), "W")
        XCTAssertEqual(echoDirection(d: .east), .east)
        XCTAssertEqual(oppositeDirection(d: .east), .west)
    }
}

