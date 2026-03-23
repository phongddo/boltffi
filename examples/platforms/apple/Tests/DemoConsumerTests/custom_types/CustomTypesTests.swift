import Demo
import Foundation
import XCTest

final class CustomTypesTests: XCTestCase {
    func testCustomTypesRoundTrip() {
        let email = "ali@example.com"
        XCTAssertEqual(echoEmail(email: email), email)
        XCTAssertEqual(emailDomain(email: email), "example.com")

        let datetime: UtcDateTime = 1_701_234_567_890
        XCTAssertEqual(echoDatetime(dt: datetime), datetime)
        XCTAssertEqual(datetimeToMillis(dt: datetime), 1_701_234_567_890)
    }
}

