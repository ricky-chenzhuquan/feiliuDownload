"""文叔叔自建夹具：匿名上传小文件 → 公开分享链接 → 验证匿名下载链（r4 轮8）"""
import json, hashlib, base64, urllib.request, time, io, os
from Crypto.Cipher import DES
from Crypto.Util.Padding import pad

UA = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/135.0.0.0 Safari/537.36 Edg/135.0.0.0"
API = "https://www.wenshushu.cn"

def b58(b: bytes) -> bytes:
    alpha = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
    n = int.from_bytes(b, "big")
    out = ""
    while n > 0:
        n, r = divmod(n, 58)
        out = alpha[r] + out
    pad0 = len(b) - len(b.lstrip(b"\x00"))
    return ("1" * pad0 + out).encode()

def req(path, payload=None, token=None, extra=None, raw=None):
    h = {"User-Agent": UA, "Content-Type": "application/json",
         "Origin": "https://www.wenshushu.cn", "Referer": "https://www.wenshushu.cn/",
         "Prod": "com.wenshushu.web.pc",
         "Priority": "u=1, i",
         "Sec-Ch-Ua": '"Microsoft Edge";v="135", "Not-A.Brand";v="8", "Chromium";v="135"',
         "Sec-Ch-Ua-Mobile": "?0",
         "Sec-Ch-Ua-Platform": '"Windows"',
         "Sec-Fetch-Dest": "empty", "Sec-Fetch-Mode": "cors", "Sec-Fetch-Site": "same-origin",
         "Accept-Language": "zh-CN, en-um;q=0.9"}
    if token: h["X-Token"] = token
    if extra: h.update(extra)
    data = raw if raw is not None else (json.dumps(payload).encode() if payload is not None else None)
    r = urllib.request.Request(API + path, data=data, headers=h)
    return json.load(urllib.request.urlopen(r, timeout=30))

# 1) 匿名 token
anon = req("/ap/login/anonymous", {"dev_info": "{}"})
token = anon["data"]["token"]
print("token ok")

# 2) 服务器时间
t = urllib.request.Request(API + "/ag/time", headers={"User-Agent": UA, "Prod": "com.wenshushu.web.pc", "Referer": "https://www.wenshushu.cn/", "Sec-Ch-Ua": '"Microsoft Edge";v="135"', "Sec-Ch-Ua-Platform": '"Windows"'})
epochtime = json.load(urllib.request.urlopen(t, timeout=20))["data"]["time"]
print("epochtime:", epochtime)

# 3) 上传一个 64KB 文件
fname = "feiliu_fixture.txt"
content = ("飞流下载 UC/网盘蒸馏夹具 r4 轮8 " + str(time.time())).encode() * 400
file_size = len(content)
print("fixture size:", file_size)

req("/ap/user/userinfo", token=token, payload={"plat": "pcweb"})
req("/ap/user/storage", token=token, payload={})
addsend_data = {
    "sender": "", "remark": "", "isextension": False,
    "notSaveTo": False, "notDownload": False, "notPreview": False,
    "downPreCountLimit": 0, "trafficStatus": 0, "pwd": "",
    "expire": "1", "recvs": ["social", "public"],
    "file_size": file_size, "file_count": 1, "fileDisplay": 0,
    "task_traffic_limit": "",
}
jd = json.dumps(addsend_data, ensure_ascii=False)
md5_hash = hashlib.md5((jd + token).encode()).hexdigest()
b58_hash = b58(md5_hash.encode())
key_iv = ("".join(epochtime[int(i)] for i in epochtime[::-1][:5]) + "000").encode()
cipher = DES.new(key_iv, DES.MODE_CBC, key_iv)
a_code = base64.b64encode(cipher.encrypt(pad(b58_hash, DES.block_size, style="pkcs7"))).decode()

r = req("/ap/task/addsend", raw=jd.encode(), token=token, extra={
    "A-code": a_code, "Prod": "com.wenshushu.web.pc", "Req-Time": epochtime})
print("addsend:", r.get("code"), r.get("message"))
data = r["data"]
bid, ufileid, tid = data["bid"], data["ufileid"], data["tid"]
print("bid/ufileid/tid:", bid[:12], ufileid[:12], tid[:12])

# 4) upId
upId = req("/ap/uploadv2/getupid", token=token, payload={
    "preid": ufileid, "boxid": bid, "linkid": tid, "utype": "sendcopy",
    "originUpid": "", "length": file_size, "count": 1})["data"]["upId"]
print("upId ok")

# 5) psurl → PUT（<2MB 单块）
ps = req("/ap/uploadv2/psurl", token=token, payload={"ispart": False, "fname": fname, "fsize": file_size, "upId": upId})
put_url = ps["data"]["url"]
pr = urllib.request.Request(put_url, data=content, headers={"Content-Type": "application/octet-stream"}, method="PUT")
urllib.request.urlopen(pr, timeout=60)
print("OSS PUT ok")

# 6) complete → copysend → 公开链接
req("/ap/uploadv2/complete", token=token, payload={
    "ispart": False, "fname": fname, "upId": upId,
    "location": {"boxid": bid, "preid": ufileid}})
for _ in range(30):
    p = req("/ap/ufile/getprocess", token=token, payload={"processId": upId})
    if (p.get("data") or {}).get("rst") == "success":
        print("process success")
        break
    time.sleep(1)
cs = req("/ap/task/copysend", token=token, payload={"bid": bid, "tid": tid, "ufileid": ufileid})
print("public_url:", cs["data"]["public_url"])
print("mgr_url:", cs["data"].get("mgr_url", "")[:60])

# 7) 验证匿名下载链（同 WsTool）：mgrtask → ufile/list → dl/sign → GET
m = req("/ap/task/mgrtask", token=token, payload={"tid": tid, "password": ""})
pid, boxid = m["data"]["ufileid"], m["data"]["boxid"]
print("mgrtask ok size:", m["data"].get("file_size"))
fl = None
for _ in range(20):
    l = req("/ap/ufile/list", token=token, payload={
        "start": 0, "sort": {"name": "asc"}, "bid": boxid, "pid": pid,
        "type": 1, "options": {"uploader": "true"}, "size": 50})
    fll = (l.get("data") or {}).get("fileList") or []
    if fll:
        fl = fll[0]; break
    time.sleep(1.5)
if not fl:
    raise SystemExit("fileList 仍为空（处理延迟）")
print("fileList:", fl["fname"], fl["fid"][:14])
s = req("/ap/dl/sign", token=token, payload={"consumeCode": 0, "type": 1, "ufileid": fl["fid"]})
durl = s["data"]["url"]
print("dl/sign url ok:", durl[:90])
got = urllib.request.urlopen(urllib.request.Request(durl, headers={"User-Agent": UA}), timeout=60).read()
print("DOWNLOAD bytes:", len(got), "match:", got == content)
open("/tmp/feiliu-r4/ws_fixture.json", "w").write(json.dumps({
    "tid": tid, "public_url": cs["data"]["public_url"], "fname": fname,
    "size": file_size, "created": time.strftime("%Y-%m-%d %H:%M")}, ensure_ascii=False))
print("FIXTURE SAVED")
