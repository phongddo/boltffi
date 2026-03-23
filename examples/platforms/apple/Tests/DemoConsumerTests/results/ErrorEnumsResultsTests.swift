import Demo
import XCTest

final class ErrorEnumsResultsTests: XCTestCase {
    func testTypedErrorResultFns() throws {
        XCTAssertEqual(try checkedDivide(a: 10, b: 2), 5)
        XCTAssertEqual(try checkedSqrt(x: 9.0), 3.0, accuracy: 1e-9)
        XCTAssertEqual(try checkedAdd(a: 2, b: 3), 5)
        XCTAssertEqual(try validateUsername(name: "valid_name"), "valid_name")

        XCTAssertThrowsError(try checkedDivide(a: 1, b: 0)) { error in
            XCTAssertEqual(error as? MathError, MathError.divisionByZero)
        }
        XCTAssertThrowsError(try checkedSqrt(x: -1.0)) { error in
            XCTAssertEqual(error as? MathError, MathError.negativeInput)
        }
        XCTAssertThrowsError(try checkedAdd(a: .max, b: 1)) { error in
            XCTAssertEqual(error as? MathError, MathError.overflow)
        }
        XCTAssertThrowsError(try validateUsername(name: "ab")) { error in
            XCTAssertEqual(error as? ValidationError, ValidationError.tooShort)
        }
        XCTAssertThrowsError(try validateUsername(name: String(repeating: "a", count: 21))) { error in
            XCTAssertEqual(error as? ValidationError, ValidationError.tooLong)
        }
        XCTAssertThrowsError(try validateUsername(name: "has space")) { error in
            XCTAssertEqual(error as? ValidationError, ValidationError.invalidFormat)
        }
    }
}

