import Demo
import XCTest

final class StreamsTests: XCTestCase {
    func testEventBusStreamsDeliverValuesAndPoints() async throws {
        let bus = EventBus()
        async let values: [Int32] = collectPrefix(bus.subscribeValues(), count: 4)
        async let points: [Point] = collectPrefix(bus.subscribePoints(), count: 2)

        try await _Concurrency.Task.sleep(nanoseconds: 100_000_000)
        bus.emitValue(value: 1)
        XCTAssertEqual(bus.emitBatch(values: [2, 3, 4]), 3)
        bus.emitPoint(point: Point(x: 1.0, y: 2.0))
        bus.emitPoint(point: Point(x: 3.0, y: 4.0))

        let emittedValues = await values
        XCTAssertEqual(emittedValues, [1, 2, 3, 4])
        let emittedPoints = await points
        XCTAssertEqual(emittedPoints, [Point(x: 1.0, y: 2.0), Point(x: 3.0, y: 4.0)])
    }
}

