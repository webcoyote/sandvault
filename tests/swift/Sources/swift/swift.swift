// The Swift Programming Language
// https://docs.swift.org/swift-book

import Algorithms

func greeting() -> String {
    "Hello world"
}

@main
struct swift {
    static func main() {
        _ = [1, 2, 3, 4].adjacentPairs().map { $0.0 + $0.1 }
        print(greeting())
    }
}
