import { useEffect, useState } from 'react';
import thornRoseAsset from '../../assets/thorn-rose.webp';
import { readMotion, readTheme, type MotionPreference, type Theme } from '../../theme';

/**
 * 简约黑白（mono）设置页右上角铜版画荆棘玫瑰暗纹（340×340，mix-blend-mode: screen）：
 * - 使用纯黑底铜版画素材 `thorn-rose.webp`，通过 `screen` 混合模式滤除纯黑底仅留银白细线；
 * - 严格仅在 `mono` 主题下挂载节点，黑黄（hud）主题完全不渲染；
 * - 挂接「05 界面与动效」设置及系统减弱动效偏好。
 */
export function ThornRoseBackdrop() {
  const [theme, setTheme] = useState<Theme>(readTheme);
  const [motion, setMotion] = useState<MotionPreference>(readMotion);

  useEffect(() => {
    const syncTheme = () => setTheme(readTheme());
    const onTheme = (event: Event) => {
      const detail = (event as CustomEvent<Theme>).detail;
      setTheme(detail ?? readTheme());
    };
    const syncMotion = () => setMotion(readMotion());
    const onMotion = (event: Event) => {
      const detail = (event as CustomEvent<MotionPreference>).detail;
      setMotion(detail ?? readMotion());
    };

    window.addEventListener('knowflow:theme', onTheme);
    window.addEventListener('knowflow:motion', onMotion);
    const onStorage = () => {
      syncTheme();
      syncMotion();
    };
    window.addEventListener('storage', onStorage);
    return () => {
      window.removeEventListener('knowflow:theme', onTheme);
      window.removeEventListener('knowflow:motion', onMotion);
      window.removeEventListener('storage', onStorage);
    };
  }, []);

  if (theme !== 'mono') {
    return null;
  }

  return (
    <img
      className="thorn-rose-backdrop gn-thorn-rose-backdrop"
      src={thornRoseAsset}
      alt=""
      aria-hidden="true"
      draggable={false}
      data-motion={motion}
    />
  );
}
