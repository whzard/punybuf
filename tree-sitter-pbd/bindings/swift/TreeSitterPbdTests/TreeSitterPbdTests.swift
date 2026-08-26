import XCTest
import SwiftTreeSitter
import TreeSitterPbd

final class TreeSitterPbdTests: XCTestCase {
    func testCanLoadGrammar() throws {
        let parser = Parser()
        let language = Language(language: tree_sitter_pbd())
        XCTAssertNoThrow(try parser.setLanguage(language),
                         "Error loading Punybuf Definition grammar")
    }
}
