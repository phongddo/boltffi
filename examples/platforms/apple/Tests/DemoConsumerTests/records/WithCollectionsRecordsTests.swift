import Demo
import XCTest

final class WithCollectionsRecordsTests: XCTestCase {
    func testPolygonFns() {
        XCTAssertEqual(echoPolygon(p: Polygon(points: [Point(x: 0, y: 0), Point(x: 1, y: 0)])), Polygon(points: [Point(x: 0, y: 0), Point(x: 1, y: 0)]))
        XCTAssertEqual(makePolygon(points: [Point(x: 0, y: 0), Point(x: 1, y: 0), Point(x: 0.5, y: 1)]).points.count, 3)
        XCTAssertEqual(polygonVertexCount(p: Polygon(points: [Point(x: 0, y: 0), Point(x: 1, y: 1)])), 2)
        assertPointEquals(polygonCentroid(p: Polygon(points: [Point(x: 0, y: 0), Point(x: 2, y: 0), Point(x: 1, y: 3)])), 1.0, 1.0, accuracy: 1e-6)
    }

    func testTeamFns() {
        XCTAssertEqual(echoTeam(t: Team(name: "QA", members: ["Dave", "Eve"])), Team(name: "QA", members: ["Dave", "Eve"]))
        XCTAssertEqual(makeTeam(name: "Dev Team", members: ["Alice", "Bob", "Charlie"]).members.count, 3)
        XCTAssertEqual(teamSize(t: Team(name: "Ops", members: ["Frank", "Grace", "Heidi", "Ivan"])), 4)
    }

    func testClassroomFns() {
        XCTAssertEqual(echoClassroom(c: Classroom(students: [Person(name: "Charlie", age: 25)])), Classroom(students: [Person(name: "Charlie", age: 25)]))
        XCTAssertEqual(makeClassroom(students: [Person(name: "Alice", age: 20), Person(name: "Bob", age: 22)]).students.count, 2)
    }

    func testTaggedScoresFns() {
        XCTAssertEqual(echoTaggedScores(ts: TaggedScores(label: "set", scores: [1.0, 2.0, 3.0])), TaggedScores(label: "set", scores: [1.0, 2.0, 3.0]))
        XCTAssertEqual(averageScore(ts: TaggedScores(label: "set", scores: [1.0, 2.0, 3.0])), 2.0, accuracy: 1e-9)
    }
}

