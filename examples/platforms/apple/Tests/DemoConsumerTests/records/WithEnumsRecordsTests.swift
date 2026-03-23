import Demo
import XCTest

final class WithEnumsRecordsTests: XCTestCase {
    func testTaskFns() {
        XCTAssertEqual(echoTask(task: Task(title: "ship", priority: .high, completed: false)), Task(title: "ship", priority: .high, completed: false))
        XCTAssertEqual(makeTask(title: "ship", priority: .critical).completed, false)
        XCTAssertEqual(isUrgent(task: Task(title: "ship", priority: .critical, completed: false)), true)
    }

    func testNotificationFns() {
        XCTAssertEqual(echoNotification(notification: Notification(message: "hello", priority: .low, read: false)), Notification(message: "hello", priority: .low, read: false))
    }
}

