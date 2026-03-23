import Demo
import XCTest

final class AsyncTraitsTests: XCTestCase {
    final class SwiftAsyncFetcher: AsyncFetcher {
        func fetchValue(key: Int32) async -> Int32 { key * 100 }
        func fetchString(input: String) async -> String { input.uppercased() }
    }

    final class SwiftAsyncOptionFetcher: AsyncOptionFetcher {
        func find(key: Int32) async -> Int64? { key > 0 ? Int64(key) * 1000 : nil }
    }

    func testAsyncTraitFns() async throws {
        let asyncFetcher = SwiftAsyncFetcher()
        let asyncOptionFetcher = SwiftAsyncOptionFetcher()

        let fetchedValue = try await fetchWithAsyncCallback(fetcher: asyncFetcher, key: 5)
        XCTAssertEqual(fetchedValue, 500)
        let fetchedString = try await fetchStringWithAsyncCallback(fetcher: asyncFetcher, input: "boltffi")
        XCTAssertEqual(fetchedString, "BOLTFFI")
        let foundValue = try await invokeAsyncOptionFetcher(fetcher: asyncOptionFetcher, key: 7)
        XCTAssertEqual(foundValue, 7_000)
        let missingValue = try await invokeAsyncOptionFetcher(fetcher: asyncOptionFetcher, key: 0)
        XCTAssertNil(missingValue)
    }
}
