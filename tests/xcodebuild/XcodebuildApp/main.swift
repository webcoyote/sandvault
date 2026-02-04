import Foundation
import Algorithms

func greeting() -> String {
    "Hello world"
}

_ = [1, 2, 3, 4].adjacentPairs().map { $0.0 + $0.1 }
print(greeting())
