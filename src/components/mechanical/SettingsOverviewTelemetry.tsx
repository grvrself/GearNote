import { useId } from 'react';

export interface SettingsOverviewTelemetryProps {
  agentReady: boolean;
  bridgeConnected: boolean;
  toolsReady: boolean;
  toolsLoading: boolean;
  hasCache: boolean;
}

type DipState = 'up' | 'mid' | 'down';

interface DipChannel {
  key: 'AGT' | 'BRG' | 'VID' | 'MEM';
  state: DipState;
}

const HEARTBEAT_PATH =
  'M 0 26 L 12 26 L 15 22 L 18 26 L 21 26 L 23 29 L 26 9 L 29 40 L 32 21 L 35 26 L 40 26 Q 43 22 46 26 ' +
  'L 56 26 L 59 23 L 62 26 L 65 26 L 67 29 L 70 11 L 73 39 L 76 22 L 79 26 L 84 26 Q 87 22 90 26 ' +
  'L 100 26 L 103 22 L 106 26 L 109 26 L 111 29 L 114 10 L 117 39 L 120 22 L 123 26 L 128 26 Q 131 23 134 26 L 144 26';

// Deterministic Code-128 style bar offsets (x, width) across 96px viewBox
const BARCODE_BARS: ReadonlyArray<readonly [number, number]> = [
  [2, 2], [6, 1], [9, 3], [14, 1], [17, 2], [21, 1], [24, 1], [27, 4],
  [33, 1], [36, 2], [40, 1], [43, 3], [48, 2], [52, 1], [55, 1], [58, 3],
  [63, 1], [66, 2], [70, 1], [73, 2], [77, 1], [80, 3], [85, 1], [88, 2], [92, 2],
];

const SCOPE_SCALE_TICKS = [6, 11, 16, 21, 26, 31, 36, 41];

export function HudCornerBrackets() {
  return (
    <span className="hud-box-corners" aria-hidden="true">
      <i /><i /><i /><i />
    </span>
  );
}

export function SettingsOverviewTelemetry({
  agentReady,
  bridgeConnected,
  toolsReady,
  toolsLoading,
  hasCache,
}: SettingsOverviewTelemetryProps) {
  const scopeGradId = `hud-scope-${useId().replace(/:/g, '')}`;

  const channels: DipChannel[] = [
    { key: 'AGT', state: agentReady ? 'up' : 'down' },
    { key: 'BRG', state: bridgeConnected ? 'up' : 'down' },
    { key: 'VID', state: toolsLoading ? 'mid' : toolsReady ? 'up' : 'down' },
    { key: 'MEM', state: hasCache ? 'up' : 'down' },
  ];

  return (
    <div className="settings-overview-telemetry" aria-hidden="true">
      <div className="hud-telemetry-unit">
        <HudCornerBrackets />
        <div className="hud-scope-wrap">
          <div className="hud-scope-screen">
            <svg
              className="hud-scope-svg"
              viewBox="0 0 144 48"
              preserveAspectRatio="none"
            >
              <defs>
                <linearGradient id={scopeGradId} x1="0%" y1="0%" x2="100%" y2="0%">
                  <stop offset="0%" stopColor="#FCEE0A" />
                  <stop offset="100%" stopColor="#00F0FF" />
                </linearGradient>
              </defs>
              <path
                className="hud-scope-wave-base"
                d={HEARTBEAT_PATH}
                fill="none"
                stroke={`url(#${scopeGradId})`}
                strokeWidth="1.2"
                strokeLinecap="round"
                strokeLinejoin="round"
              />
              <path
                className="hud-scope-wave"
                d={HEARTBEAT_PATH}
                fill="none"
                stroke={`url(#${scopeGradId})`}
                strokeWidth="1.2"
                strokeLinecap="round"
                strokeLinejoin="round"
              />
            </svg>
          </div>
          <svg className="hud-scope-scale" viewBox="0 0 8 48" aria-hidden="true">
            <line x1="2" y1="4" x2="2" y2="44" stroke="rgba(255, 255, 255, 0.14)" strokeWidth="1" />
            {SCOPE_SCALE_TICKS.map((y, index) => (
              <line
                key={y}
                x1="2"
                y1={y}
                x2={index % 2 === 0 ? 7 : 5}
                y2={y}
                stroke={index === 4 ? 'rgba(252, 238, 10, 0.45)' : 'rgba(255, 255, 255, 0.24)'}
                strokeWidth="1"
              />
            ))}
          </svg>
        </div>

        <div className="hud-dip-bank">
          {channels.map((channel) => (
            <div
              key={channel.key}
              className="hud-dip-col"
              data-channel={channel.key}
              data-state={channel.state}
            >
              <span className="hud-dip-label">{channel.key}</span>
              <span className="hud-dip-slot">
                <span className="hud-dip-cap" />
              </span>
              <span className="hud-dip-dot" />
            </div>
          ))}
        </div>
      </div>

      <div className="hud-barcode-plate">
        <HudCornerBrackets />
        <svg
          className="hud-barcode-svg"
          viewBox="0 0 96 24"
          shapeRendering="crispEdges"
        >
          {BARCODE_BARS.map(([x, width], idx) => (
            <rect
              key={`${x}-${idx}`}
              x={x}
              y={0}
              width={width}
              height={24}
              fill="rgba(255, 255, 255, 0.45)"
            />
          ))}
        </svg>
        <span className="hud-barcode-code gn-tabular">GN-001 // LOCAL</span>
      </div>
    </div>
  );
}
