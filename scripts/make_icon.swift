// 飞流下载图标生成：渐变圆角方块 + 白色闪电
// 用法: swift make_icon.swift /tmp/feiliu-icon.png
import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

let size = 1024
let outPath = CommandLine.arguments.count > 1
    ? CommandLine.arguments[1]
    : "feiliu-icon.png"

let space = CGColorSpace(name: CGColorSpace.sRGB)!
let ctx = CGContext(
    data: nil, width: size, height: size,
    bitsPerComponent: 8, bytesPerRow: 0, space: space,
    bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
)!

// 背景圆角方块（留出 macOS 要求的透明边距感，整体铺满由 tauri icon 处理）
let rect = CGRect(x: 0, y: 0, width: size, height: size)
let radius: CGFloat = 232
let bg = CGPath(roundedRect: rect, cornerWidth: radius, cornerHeight: radius, transform: nil)
ctx.addPath(bg)
ctx.clip()

let colors = [
    CGColor(red: 0.29, green: 0.52, blue: 1.00, alpha: 1),
    CGColor(red: 0.10, green: 0.80, blue: 0.93, alpha: 1),
] as CFArray
let grad = CGGradient(colorsSpace: space, colors: colors, locations: [0, 1])!
ctx.drawLinearGradient(grad, start: CGPoint(x: 0, y: CGFloat(size)), end: CGPoint(x: CGFloat(size), y: 0), options: [])

// 顶部高光
let sheen = CGGradient(colorsSpace: space, colors: [
    CGColor(red: 1, green: 1, blue: 1, alpha: 0.22),
    CGColor(red: 1, green: 1, blue: 1, alpha: 0),
] as CFArray, locations: [0, 1])!
ctx.drawLinearGradient(sheen, start: CGPoint(x: CGFloat(size) / 2, y: CGFloat(size)), end: CGPoint(x: CGFloat(size) / 2, y: 0), options: [])

// 白色闪电（居中偏右下）
let bolt = CGMutablePath()
bolt.move(to: CGPoint(x: 565, y: 170))
bolt.addLine(to: CGPoint(x: 300, y: 580))
bolt.addLine(to: CGPoint(x: 475, y: 580))
bolt.addLine(to: CGPoint(x: 420, y: 850))
bolt.addLine(to: CGPoint(x: 700, y: 470))
bolt.addLine(to: CGPoint(x: 520, y: 470))
bolt.closeSubpath()

// 左上速度线
ctx.setFillColor(CGColor(red: 1, green: 1, blue: 1, alpha: 0.55))
ctx.fill(CGRect(x: 150, y: 800, width: 190, height: 42))
ctx.setFillColor(CGColor(red: 1, green: 1, blue: 1, alpha: 0.35))
ctx.fill(CGRect(x: 110, y: 720, width: 130, height: 42))

// 闪电阴影 + 主体
ctx.setShadow(offset: CGSize(width: 0, height: -14), blur: 44, color: CGColor(red: 0, green: 0.1, blue: 0.3, alpha: 0.4))
ctx.setFillColor(CGColor(red: 1, green: 1, blue: 1, alpha: 1))
ctx.addPath(bolt)
ctx.fillPath()

let image = ctx.makeImage()!
let out = URL(fileURLWithPath: outPath)
let dest = CGImageDestinationCreateWithURL(out as CFURL, UTType.png.identifier as CFString, 1, nil)!
CGImageDestinationAddImage(dest, image, nil)
if !CGImageDestinationFinalize(dest) {
    fputs("failed to write \(outPath)\n", stderr)
    exit(1)
}
print("icon written to \(outPath)")
