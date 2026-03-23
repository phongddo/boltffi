import Demo
import XCTest

final class ReprIntEnumsTests: XCTestCase {
    func testPriorityFns() {
        XCTAssertEqual(echoPriority(p: Priority.high), Priority.high)
        XCTAssertEqual(priorityLabel(p: Priority.low), "low")
        XCTAssertEqual(isHighPriority(p: Priority.critical), true)
        XCTAssertEqual(isHighPriority(p: Priority.low), false)
    }

    func testLogLevelFns() {
        XCTAssertEqual(echoLogLevel(level: LogLevel.info), LogLevel.info)
        XCTAssertEqual(shouldLog(level: LogLevel.error, minLevel: LogLevel.warn), true)
        XCTAssertEqual(echoVecLogLevel(levels: [LogLevel.trace, LogLevel.info, LogLevel.error]), [LogLevel.trace, LogLevel.info, LogLevel.error])
    }
}
