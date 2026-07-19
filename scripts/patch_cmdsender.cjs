const fs = require('fs');
let c = fs.readFileSync('src/components/displays/CommandSender.tsx', 'utf8');

// Add loopback settings panel before the closing </div></div> of the sidebar
const loopbackPanel = `
        {/* 回环模式设置 */}
        <div className="text-[10px] text-text-secondary uppercase tracking-wide font-semibold pt-2">{t(lang, 'cmdLoopback')}</div>
        <div className="flex flex-col gap-2 p-2 bg-bg-editor border border-border rounded">
          <div className="grid grid-cols-[80px_1fr] items-center gap-2">
            <label className="text-xs text-text-secondary">{t(lang, 'cmdLoopback')}</label>
            <button
              className={\`bg-bg-input border border-border text-text-secondary px-2 py-0.5 text-xs rounded-sm cursor-pointer transition-all hover:text-text-primary \${params.loopbackEnabled ? 'bg-bg-button text-text-inverse border-bg-button' : ''}\`}
              onClick={() => updateParams({ loopbackEnabled: !params.loopbackEnabled })}
            >
              {params.loopbackEnabled ? t(lang, 'cmdNewlineOn') : t(lang, 'cmdNewlineOff')}
            </button>
          </div>
          {params.loopbackEnabled && (
            <>
              <div className="grid grid-cols-[80px_1fr] items-center gap-2">
                <label className="text-xs text-text-secondary">{t(lang, 'cmdLoopbackManual')}</label>
                <select
                  value={params.loopbackSendMode}
                  onChange={(e) => updateParams({ loopbackSendMode: e.target.value as 'manual' | 'onChange' | 'timer' })}
                  className="text-xs w-full px-2 py-1 bg-bg-input text-text-primary border border-border rounded focus:outline-none focus:border-accent"
                >
                  <option value="manual">{t(lang, 'cmdLoopbackManual')}</option>
                  <option value="onChange">{t(lang, 'cmdLoopbackOnChange')}</option>
                  <option value="timer">{t(lang, 'cmdLoopbackTimer')}</option>
                </select>
              </div>
              {params.loopbackSendMode === 'timer' && (
                <div className="grid grid-cols-[80px_1fr] items-center gap-2">
                  <label className="text-xs text-text-secondary">{t(lang, 'cmdLoopbackInterval')}</label>
                  <input
                    type="number"
                    min={10}
                    max={10000}
                    value={params.loopbackTimerMs}
                    onChange={(e) => updateParams({ loopbackTimerMs: parseInt(e.target.value) || 100 })}
                    className="text-xs w-full px-2 py-1 bg-bg-input text-text-primary border border-border rounded focus:outline-none focus:border-accent"
                  />
                </div>
              )}
            </>
          )}
        </div>`;

// Insert before the last two closing divs of the sidebar
c = c.replace(
  '        </div>\n      </div>\n    </div>\n  );\n}',
  loopbackPanel + '\n      </div>\n    </div>\n  );\n}'
);

fs.writeFileSync('src/components/displays/CommandSender.tsx', c);
console.log('done');
