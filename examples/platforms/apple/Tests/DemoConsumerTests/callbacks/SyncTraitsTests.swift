import Demo
import XCTest

final class SyncTraitsTests: XCTestCase {
    final class Doubler: ValueCallback {
        func onValue(value: Int32) -> Int32 { value * 2 }
    }

    final class Tripler: ValueCallback {
        func onValue(value: Int32) -> Int32 { value * 3 }
    }

    final class SwiftPointTransformer: PointTransformer {
        func transform(point: Point) -> Point { Point(x: point.x + 10.0, y: point.y + 20.0) }
    }

    final class SwiftStatusMapper: StatusMapper {
        func mapStatus(status: Status) -> Status { status == .pending ? .active : .inactive }
    }

    final class SwiftVecProcessor: VecProcessor {
        func process(values: [Int32]) -> [Int32] { values.map { $0 * $0 } }
    }

    final class SwiftMultiMethodCallback: MultiMethodCallback {
        func methodA(x: Int32) -> Int32 { x + 1 }
        func methodB(x: Int32, y: Int32) -> Int32 { x * y }
        func methodC() -> Int32 { 5 }
    }

    final class SwiftOptionCallback: OptionCallback {
        func findValue(key: Int32) -> Int32? { key > 0 ? key * 10 : nil }
    }

    func testSyncTraitFns() {
        let doubler = Doubler()
        let tripler = Tripler()
        let pointTransformer = SwiftPointTransformer()
        let statusMapper = SwiftStatusMapper()
        let multiMethod = SwiftMultiMethodCallback()
        let optionCallback = SwiftOptionCallback()
        let vecProcessor = SwiftVecProcessor()

        XCTAssertEqual(invokeValueCallback(callback: doubler, input: 4), 8)
        XCTAssertEqual(invokeValueCallbackTwice(callback: doubler, a: 3, b: 4), 14)
        XCTAssertEqual(invokeBoxedValueCallback(callback: doubler, input: 5), 10)
        XCTAssertEqual(transformPoint(transformer: pointTransformer, point: Point(x: 1.0, y: 2.0)), Point(x: 11.0, y: 22.0))
        XCTAssertEqual(transformPointBoxed(transformer: pointTransformer, point: Point(x: 3.0, y: 4.0)), Point(x: 13.0, y: 24.0))
        XCTAssertEqual(mapStatus(mapper: statusMapper, status: .pending), .active)
        XCTAssertEqual(processVec(processor: vecProcessor, values: [1, 2, 3]), [1, 4, 9])
        XCTAssertEqual(invokeMultiMethod(callback: multiMethod, x: 3, y: 4), 21)
        XCTAssertEqual(invokeMultiMethodBoxed(callback: multiMethod, x: 3, y: 4), 21)
        XCTAssertEqual(invokeTwoCallbacks(first: doubler, second: tripler, value: 5), 25)
        XCTAssertEqual(invokeOptionCallback(callback: optionCallback, key: 7), 70)
        XCTAssertNil(invokeOptionCallback(callback: optionCallback, key: 0))
    }
}

