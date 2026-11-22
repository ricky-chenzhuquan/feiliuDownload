import { useCallback, useEffect, useState } from "react";

/**
 * 网盘登录卡片通用逻辑：登录状态轮询 + 登录/退出动作 + 消息反馈。
 * statusFn/loginFn/logoutFn 为 api.ts 的三绑定（百度/夸克/123 云盘同构）。
 */
export function useCloudLogin(
  statusFn: () => Promise<{ logged_in: boolean; saved_at: number | null }>,
  loginFn: () => Promise<string>,
  logoutFn: () => Promise<void>,
) {
  const [status, setStatus] = useState<{
    logged_in: boolean;
    saved_at: number | null;
  } | null>(null);
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);

  useEffect(() => {
    statusFn().then(setStatus).catch(() => {});
  }, [statusFn]);

  const refresh = useCallback(() => {
    statusFn().then(setStatus).catch(() => {});
  }, [statusFn]);

  const login = useCallback(async () => {
    setBusy(true);
    setMsg(null);
    try {
      const m = await loginFn();
      setMsg(m);
      refresh();
    } catch (e) {
      setMsg(String(e));
    } finally {
      setBusy(false);
    }
  }, [loginFn, refresh]);

  const logout = useCallback(async () => {
    setBusy(true);
    try {
      await logoutFn();
      setMsg(null);
      refresh();
    } finally {
      setBusy(false);
    }
  }, [logoutFn, refresh]);

  return { status, busy, msg, login, logout };
}

/** 网盘登录卡片（设置页）：百度/夸克/123 云盘三站同构 UI */
export function LoginCard({
  title,
  loggedIn,
  savedAt,
  busy,
  msg,
  onLogin,
  onLogout,
  loginLabel,
  hint,
}: {
  title: string;
  loggedIn: boolean;
  savedAt: number | null;
  busy: boolean;
  msg: string | null;
  onLogin: () => void;
  onLogout: () => void;
  loginLabel: string;
  hint: string;
}) {
  return (
    <div className="settings-card muted">
      <h3>{title}</h3>
      {loggedIn ? (
        <div className="update-row">
          <span className="upd-ok">
            已登录 ✓（
            {savedAt ? new Date(savedAt).toLocaleString() : "会话有效"}）
          </span>
          <button className="btn ghost" onClick={onLogout} disabled={busy}>
            退出登录
          </button>
        </div>
      ) : (
        <div className="update-row">
          <span className="dim">未登录</span>
          <button className="btn primary" onClick={onLogin} disabled={busy}>
            {busy ? "等待登录…" : loginLabel}
          </button>
        </div>
      )}
      {msg && <p className="dim-note">{msg}</p>}
      <p className="dim-note">{hint}</p>
    </div>
  );
}
