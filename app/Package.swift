// swift-tools-version: 6.0

import PackageDescription

// Application graphique de macfind. Elle ne contient aucune logique réseau :
// elle lance le binaire `macfind` (embarqué dans le bundle) avec `--json`.
let package = Package(
    name: "MacfindApp",
    platforms: [.macOS(.v14)],
    products: [
        .executable(name: "MacfindApp", targets: ["MacfindApp"])
    ],
    targets: [
        .executableTarget(name: "MacfindApp"),
        .testTarget(name: "MacfindAppTests", dependencies: ["MacfindApp"]),
    ],
    swiftLanguageModes: [.v5]
)
