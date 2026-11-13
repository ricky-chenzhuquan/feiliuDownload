#!/usr/bin/env node
/**
 * 生成 Tauri 更新清单 latest.json。
 * 用法: node gen-latest-json.js <artifacts-root-dir> <tag> <owner/repo>
 *
 * 扫描构建产物中的签名更新包（*.app.tar.gz / *-setup.exe 及其 .sig），
 * 生成 { version, notes, pub_date, platforms } 写入 <dir>/latest.json。
 * 平台 key 映射: aarch64.app.tar.gz → darwin-aarch64；x64-setup.exe → windows-x86_64
 */
const fs = require("fs");
const path = require("path");

const [root, tag, repo] = process.argv.slice(2);
if (!root || !tag || !repo) {
  console.error("usage: node gen-latest-json.js <dir> <tag> <owner/repo>");
  process.exit(1);
}

// 递归收集文件
function walk(dir, acc = []) {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p, acc);
    else acc.push(p);
  }
  return acc;
}

const files = walk(root);
const version = tag.replace(/^v/, "");
const base = `https://github.com/${repo}/releases/download/${tag}`;

function findAsset(suffix) {
  // 精确匹配产物名；找不到时退回宽松匹配
  return (
    files.find((f) => f.endsWith(suffix)) ||
    files.find((f) => path.basename(f).includes(suffix.replace(/^FeiLiu_/, "")))
  );
}

const platforms = {};

function addPlatform(key, assetSuffix, sigSuffix) {
  const asset = findAsset(assetSuffix);
  const sig = findAsset(sigSuffix);
  if (!asset || !sig) {
    if (asset || sig) console.warn(`warning: ${key} 缺少配对的资产/签名文件`);
    return;
  }
  platforms[key] = {
    signature: fs.readFileSync(sig, "utf8").trim(),
    url: `${base}/${path.basename(asset)}`,
  };
}

addPlatform("darwin-aarch64", ".app.tar.gz", ".app.tar.gz.sig");
addPlatform("windows-x86_64", "x64-setup.exe", "x64-setup.exe.sig");

const manifest = {
  version,
  notes: `FeiLiu Downloader ${version}`,
  pub_date: new Date().toISOString(),
  platforms,
};

const out = path.join(root, "latest.json");
fs.writeFileSync(out, JSON.stringify(manifest, null, 2));
console.log(`latest.json written: ${out}`);
console.log(JSON.stringify(manifest, null, 2));
