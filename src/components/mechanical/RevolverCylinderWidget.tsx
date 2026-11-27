import { useEffect, useId, useMemo, useState, type CSSProperties, type KeyboardEvent } from 'react';
import { readMotion, type MotionPreference } from '../../theme';

type Props = {
  /** 当前激活的设置分组索引（0～7） */
  activeIndex: number;
};

const CX = 90;
const CY = 90;
const OUTER_R = 66;
const CHAMBER_RING_R = 40;
const CHAMBER_OUTER_R = 15.5;
const CHAMBER_INNER_R = 13.5;
const FLUTE_ARC_R = 17.5;
const STEP_DEG = 60;

const round = (value: number) => Number(value.toFixed(2));
const clockRad = (degCwFromTop: number) => ((degCwFromTop - 90) * Math.PI) / 180;

/**
 * 构建经典的带 6 处外缘半圆凹槽的左轮弹仓外轮廓（Fluted Cylinder，R=66）：
 * 在 30° / 90° / 150° / 210° / 270° / 330°（顺时针距正上方）位置向内凹切凹槽。
 */
function buildFlutedCylinderPath(): { outerContour: string; fluteBevels: string[] } {
  const parts: string[] = [];
  const bevels: string[] = [];
  const landHalfDeg = 18.5;

  for (let i = 0; i < 6; i++) {
    const chamberDeg = i * 60;
    const landStartDeg = chamberDeg - landHalfDeg;
    const landEndDeg = chamberDeg + landHalfDeg;
    const nextLandStartDeg = (i + 1) * 60 - landHalfDeg;

    const sx = round(CX + Math.cos(clockRad(landStartDeg)) * OUTER_R);
    const sy = round(CY + Math.sin(clockRad(landStartDeg)) * OUTER_R);
    const ex = round(CX + Math.cos(clockRad(landEndDeg)) * OUTER_R);
    const ey = round(CY + Math.sin(clockRad(landEndDeg)) * OUTER_R);
    const nx = round(CX + Math.cos(clockRad(nextLandStartDeg)) * OUTER_R);
    const ny = round(CY + Math.sin(clockRad(nextLandStartDeg)) * OUTER_R);

    if (i === 0) {
      parts.push(`M ${sx} ${sy}`);
    }
    // Convex outer cylinder rim over chamber i
    parts.push(`A ${OUTER_R} ${OUTER_R} 0 0 1 ${ex} ${ey}`);
    // Concave milled flute cutout between chamber i and chamber i+1
    parts.push(`A ${FLUTE_ARC_R} ${FLUTE_ARC_R} 0 0 0 ${nx} ${ny}`);

    // Delicate inner concentric chamfer line along each chamber land
    const bsx = round(CX + Math.cos(clockRad(chamberDeg - 14)) * (OUTER_R - 3));
    const bsy = round(CY + Math.sin(clockRad(chamberDeg - 14)) * (OUTER_R - 3));
    const bex = round(CX + Math.cos(clockRad(chamberDeg + 14)) * (OUTER_R - 3));
    const bey = round(CY + Math.sin(clockRad(chamberDeg + 14)) * (OUTER_R - 3));
    bevels.push(`M ${bsx} ${bsy} A ${OUTER_R - 3} ${OUTER_R - 3} 0 0 1 ${bex} ${bey}`);
  }

  parts.push('Z');
  return { outerContour: parts.join(' '), fluteBevels: bevels };
}

/** 构建中心 6 齿星形棘轮（r = 13.5 ～ 18.5） */
function buildStarRatchetPath(): string {
  const pts: string[] = [];
  for (let i = 0; i < 6; i++) {
    const toothTipDeg = i * 60;
    const valleyDeg = i * 60 + 26;
    const shoulderDeg = i * 60 + 48;

    const tRad = clockRad(toothTipDeg);
    const vRad = clockRad(valleyDeg);
    const sRad = clockRad(shoulderDeg);

    const tx = round(CX + Math.cos(tRad) * 18.2);
    const ty = round(CY + Math.sin(tRad) * 18.2);
    const vx = round(CX + Math.cos(vRad) * 13.6);
    const vy = round(CY + Math.sin(vRad) * 13.6);
    const sx = round(CX + Math.cos(sRad) * 15.8);
    const sy = round(CY + Math.sin(sRad) * 15.8);

    pts.push(`${i === 0 ? 'M' : 'L'} ${tx} ${ty}`);
    pts.push(`L ${vx} ${vy}`);
    pts.push(`L ${sx} ${sy}`);
  }
  pts.push('Z');
  return pts.join(' ');
}

/**
 * 简约黑白（mono）主题左下角 CAD 精密 6 孔左轮弹仓组件（175px 高度，#0B0B0D）：
 * - 静态基准：微弱十字中心轴线 + 正上方 12 点位倒三角准星标记；
 * - 旋转弹仓：带 6 处凹槽的外轮廓（R=66）、6 颗双圈同心弹孔（R=40）、星形棘轮轴；
 * - 交互逻辑：左侧分组每次切换 +60deg 顺时针步进，点击弹仓也可触发 +60deg 棘轮拨动。
 */
export function RevolverCylinderWidget({ activeIndex }: Props) {
  const glowId = `revolver-dot-glow-${useId().replace(/:/g, '')}`;
  const [motion, setMotion] = useState<MotionPreference>(readMotion);
  const [prevActiveIndex, setPrevActiveIndex] = useState(activeIndex);
  const [steps, setSteps] = useState(0);

  if (activeIndex !== prevActiveIndex) {
    setPrevActiveIndex(activeIndex);
    setSteps((prev) => prev + 1);
  }

  useEffect(() => {
    const syncMotion = () => setMotion(readMotion());
    const onMotion = (event: Event) => {
      const detail = (event as CustomEvent<MotionPreference>).detail;
      setMotion(detail ?? readMotion());
    };
    window.addEventListener('knowflow:motion', onMotion);
    window.addEventListener('storage', syncMotion);
    return () => {
      window.removeEventListener('knowflow:motion', onMotion);
      window.removeEventListener('storage', syncMotion);
    };
  }, []);

  const geometry = useMemo(() => {
    const { outerContour, fluteBevels } = buildFlutedCylinderPath();
    const starRatchet = buildStarRatchetPath();
    const chambers = Array.from({ length: 6 }, (_, index) => {
      const angle = clockRad(index * 60);
      return {
        index,
        cx: round(CX + Math.cos(angle) * CHAMBER_RING_R),
        cy: round(CY + Math.sin(angle) * CHAMBER_RING_R),
      };
    });
    return { outerContour, fluteBevels, starRatchet, chambers };
  }, []);

  // 子弹实体固定装填于 0 号物理弹仓，随弹仓每步 +60deg（顶部由左向右顺时针）一同步进
  const loadedChamber = 0;
  const topChamberIndex = (6 - (steps % 6)) % 6;
  const bulletClockPosition = steps % 6;
  const cylinderStyle = {
    '--cylinder-angle': `${steps * STEP_DEG}deg`,
  } as CSSProperties;

  const spinOnce = () => setSteps((prev) => prev + 1);
  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      spinOnce();
    }
  };

  return (
    <div
      className="revolver-cylinder-widget"
      role="button"
      tabIndex={0}
      aria-label="CAD 六孔左轮弹仓组件，点击顺时针旋转"
      data-steps={steps}
      data-active-chamber={loadedChamber}
      data-top-chamber={topChamberIndex}
      data-bullet-pos={bulletClockPosition}
      data-motion={motion}
      onClick={spinOnce}
      onKeyDown={handleKeyDown}
    >
      <span className="revolver-serial gn-tabular">CYL-06 // CHAMBER</span>

      <span className="revolver-corner revolver-corner--tl" aria-hidden="true" />
      <span className="revolver-corner revolver-corner--tr" aria-hidden="true" />
      <span className="revolver-corner revolver-corner--bl" aria-hidden="true" />
      <span className="revolver-corner revolver-corner--br" aria-hidden="true" />

      <div className="revolver-stage" aria-hidden="true">
        <svg className="revolver-svg" viewBox="0 0 180 180" fill="none">
          <defs>
            <filter id={glowId} x="-60%" y="-60%" width="220%" height="220%">
              <feGaussianBlur stdDeviation="1.6" result="blur" />
              <feMerge>
                <feMergeNode in="blur" />
                <feMergeNode in="SourceGraphic" />
              </feMerge>
            </filter>
          </defs>

          {/* 静态基准：十字中心轴线与外周极细参考环 */}
          <line x1="12" y1="90" x2="168" y2="90" stroke="rgba(255, 255, 255, 0.08)" strokeWidth="0.7" />
          <line x1="90" y1="16" x2="90" y2="168" stroke="rgba(255, 255, 255, 0.08)" strokeWidth="0.7" />
          <circle
            cx="90"
            cy="90"
            r="71"
            stroke="rgba(255, 255, 255, 0.07)"
            strokeWidth="0.65"
            strokeDasharray="2 3"
          />

          {/* 正上方 12 点位倒三角准星标记 */}
          <path d="M 86.5 10.5 L 93.5 10.5 L 90 16.5 Z" fill="rgba(255, 255, 255, 0.86)" />

          {/* 可旋转弹仓本体 */}
          <g className="revolver-cylinder-rotor" style={cylinderStyle}>
            {/* 6 处凹槽经典外轮廓 */}
            <path d={geometry.outerContour} stroke="rgba(255, 255, 255, 0.38)" strokeWidth="1" />
            {geometry.fluteBevels.map((d, idx) => (
              <path key={`bevel-${idx}`} d={d} stroke="rgba(255, 255, 255, 0.14)" strokeWidth="0.6" />
            ))}

            {/* 弹孔分布参考细圆 */}
            <circle cx="90" cy="90" r={CHAMBER_RING_R} stroke="rgba(255, 255, 255, 0.08)" strokeWidth="0.6" />

            {/* 6 颗精密双层同心弹孔 */}
            {geometry.chambers.map((chamber) => {
              const hasBullet = chamber.index === loadedChamber;
              return (
                <g key={chamber.index} className="revolver-chamber" data-loaded={hasBullet ? 'true' : 'false'}>
                  <circle
                    cx={chamber.cx}
                    cy={chamber.cy}
                    r={CHAMBER_OUTER_R}
                    fill={hasBullet ? 'rgba(255, 255, 255, 0.12)' : 'none'}
                    stroke={hasBullet ? 'rgba(255, 255, 255, 0.56)' : 'rgba(255, 255, 255, 0.34)'}
                    strokeWidth={hasBullet ? 1.05 : 0.9}
                  />
                  <circle
                    cx={chamber.cx}
                    cy={chamber.cy}
                    r={CHAMBER_INNER_R}
                    stroke={hasBullet ? 'rgba(255, 255, 255, 0.34)' : 'rgba(255, 255, 255, 0.22)'}
                    strokeWidth="0.65"
                  />
                  {hasBullet ? (
                    <circle
                      className="revolver-active-pin"
                      cx={chamber.cx}
                      cy={chamber.cy}
                      r="2.6"
                      fill="#FFFFFF"
                      filter={`url(#${glowId})`}
                    />
                  ) : (
                    <circle
                      cx={chamber.cx}
                      cy={chamber.cy}
                      r="2.4"
                      stroke="rgba(255, 255, 255, 0.14)"
                      strokeWidth="0.55"
                    />
                  )}
                </g>
              );
            })}

            {/* 中心棘轮星形轴 */}
            <circle cx="90" cy="90" r="22.5" stroke="rgba(255, 255, 255, 0.12)" strokeWidth="0.6" />
            <path d={geometry.starRatchet} stroke="rgba(255, 255, 255, 0.36)" strokeWidth="0.85" />
            <circle cx="90" cy="90" r="11.5" stroke="rgba(255, 255, 255, 0.20)" strokeWidth="0.65" />
            <circle cx="90" cy="90" r="7" stroke="rgba(255, 255, 255, 0.44)" strokeWidth="0.95" />
            <circle cx="90" cy="90" r="3" stroke="rgba(255, 255, 255, 0.24)" strokeWidth="0.6" />
          </g>
        </svg>
      </div>
    </div>
  );
}
