import { describe, it, expect } from 'vitest';
import {
  flattenSections,
  filterSections,
  sectionAnchors,
  sectionAtScroll,
  totalSizeOf,
  HEADER_SIZE,
  ROW_SIZE,
  type PaletteSection,
} from '../paletteModel';

/// 测试用分组 — 3 个分组共 5 行
const sections: PaletteSection[] = [
  {
    id: 'input',
    header: 'Input',
    category: 'input',
    entries: [
      { key: 'Knob', icon: null, label: 'Knob', title: 'Knob' },
      { key: 'Button', icon: null, label: 'Button', title: 'Button' },
    ],
  },
  {
    id: 'display',
    header: 'Display',
    category: 'display',
    entries: [{ key: 'Waveform', icon: null, label: 'Waveform', title: 'Waveform' }],
  },
  {
    id: 'math',
    header: 'Math',
    category: 'math',
    entries: [
      { key: 'add', icon: null, label: 'Add', title: 'Add' },
      { key: 'sub', icon: null, label: 'Sub', title: 'Sub' },
    ],
  },
];

describe('flattenSections', () => {
  it('全部展开时按顺序输出 header 与行', () => {
    const items = flattenSections(sections, {});
    expect(items.map((i) => i.key)).toEqual([
      'h:input',
      'r:Knob',
      'r:Button',
      'h:display',
      'r:Waveform',
      'h:math',
      'r:add',
      'r:sub',
    ]);
  });

  it('每个条目携带所属分组 id 与分类', () => {
    const items = flattenSections(sections, {});
    expect(items[1]).toMatchObject({ type: 'row', sectionId: 'input', category: 'input' });
    expect(items[4]).toMatchObject({ type: 'row', sectionId: 'display', category: 'display' });
    expect(items[5]).toMatchObject({ type: 'header', sectionId: 'math', header: 'Math' });
  });

  it('折叠分组仅保留分组头', () => {
    const items = flattenSections(sections, { display: true });
    expect(items.map((i) => i.key)).toEqual([
      'h:input',
      'r:Knob',
      'r:Button',
      'h:display',
      'h:math',
      'r:add',
      'r:sub',
    ]);
  });

  it('全部折叠时只剩分组头', () => {
    const items = flattenSections(sections, { input: true, display: true, math: true });
    expect(items.map((i) => i.key)).toEqual(['h:input', 'h:display', 'h:math']);
  });

  it('条目 key 全局唯一', () => {
    const items = flattenSections(sections, {});
    expect(new Set(items.map((i) => i.key)).size).toBe(items.length);
  });
});

describe('sectionAnchors', () => {
  it('按固定行高累计各分组 header 的像素偏移', () => {
    const items = flattenSections(sections, {});
    expect(sectionAnchors(items)).toEqual([
      { id: 'input', offset: 0 },
      { id: 'display', offset: HEADER_SIZE + ROW_SIZE * 2 },
      { id: 'math', offset: HEADER_SIZE * 2 + ROW_SIZE * 3 },
    ]);
  });

  it('折叠分组的行不参与累计', () => {
    const items = flattenSections(sections, { input: true });
    expect(sectionAnchors(items)).toEqual([
      { id: 'input', offset: 0 },
      { id: 'display', offset: HEADER_SIZE },
      { id: 'math', offset: HEADER_SIZE * 2 + ROW_SIZE },
    ]);
  });
});

describe('totalSizeOf', () => {
  it('等于全部分组头与行的高度之和', () => {
    const items = flattenSections(sections, {});
    expect(totalSizeOf(items)).toBe(HEADER_SIZE * 3 + ROW_SIZE * 5);
  });

  it('与最后一个锚点加剩余条目高度一致', () => {
    const items = flattenSections(sections, { display: true });
    const anchors = sectionAnchors(items);
    const last = anchors[anchors.length - 1];
    expect(totalSizeOf(items)).toBe(last.offset + HEADER_SIZE + ROW_SIZE * 2);
  });
});

describe('sectionAtScroll', () => {
  const anchors = sectionAnchors(flattenSections(sections, {}));
  const displayOffset = HEADER_SIZE + ROW_SIZE * 2;

  it('顶部时属于第一个分组', () => {
    expect(sectionAtScroll(anchors, 0)).toBe('input');
  });

  it('滚过某分组 header 后属于该分组', () => {
    expect(sectionAtScroll(anchors, displayOffset)).toBe('display');
    expect(sectionAtScroll(anchors, displayOffset + 10)).toBe('display');
  });

  it('分组头进入顶部 slack 余量内即算进入该分组', () => {
    expect(sectionAtScroll(anchors, displayOffset - 32)).toBe('display');
    expect(sectionAtScroll(anchors, displayOffset - 33)).toBe('input');
  });

  it('空锚点回退到第一个分组', () => {
    expect(sectionAtScroll([], 100)).toBe('input');
  });
});

describe('filterSections', () => {
  it('空查询直接返回原 sections (内容相等)', () => {
    const out = filterSections(sections, '');
    expect(out).toEqual(sections);
  });

  it('全空白查询也视为空', () => {
    const out = filterSections(sections, '   ');
    expect(out).toEqual(sections);
  });

  it('按 label 子串大小写不敏感匹配', () => {
    const out = filterSections(sections, 'kn');
    expect(out).toHaveLength(1);
    expect(out[0]?.entries.map((e) => e.key)).toEqual(['Knob']);
  });

  it('按 title 匹配, 不只匹配 label', () => {
    // 给 Button 一个不同的 title
    const sectionsWithTitle: PaletteSection[] = [
      {
        id: 'input',
        header: 'Input',
        category: 'input',
        entries: [
          { key: 'Knob', icon: null, label: 'Knob', title: '旋钮' },
          { key: 'Button', icon: null, label: 'Button', title: '按钮' },
        ],
      },
    ];
    const out = filterSections(sectionsWithTitle, '旋钮');
    expect(out[0]?.entries.map((e) => e.key)).toEqual(['Knob']);
    const out2 = filterSections(sectionsWithTitle, '按钮');
    expect(out2[0]?.entries.map((e) => e.key)).toEqual(['Button']);
  });

  it('全空分组被剔除 (无匹配)', () => {
    const out = filterSections(sections, 'xyz-nothing');
    expect(out).toEqual([]);
  });

  it('保留所有至少有一条命中的分组 (大小写不敏感)', () => {
    // 'b' (lowercased 'B') 出现在 Knob/Button/sub 中
    const out = filterSections(sections, 'B');
    const allKeys = out.flatMap((s) => s.entries.map((e) => e.key));
    expect(allKeys).toEqual(expect.arrayContaining(['Knob', 'Button', 'sub']));
    expect(allKeys).not.toContain('Waveform');
    expect(allKeys).not.toContain('add');
  });

  it('返回的 sections 不引用原数组 (filter 副作用隔离)', () => {
    const out = filterSections(sections, 'K');
    expect(out).not.toBe(sections);
  });

  it('跨分组命中: "K" 仅命中 Knob (大写敏感测试)', () => {
    const out = filterSections(sections, 'K');
    const allKeys = out.flatMap((s) => s.entries.map((e) => e.key));
    expect(allKeys).toEqual(['Knob']);
    expect(out).toHaveLength(1);
    expect(out[0]?.id).toBe('input');
  });

  it('不修改原 sections 的引用 (immutable)', () => {
    const before = sections.map((s) => ({ ...s, entries: [...s.entries] }));
    filterSections(sections, 'kn');
    expect(sections).toEqual(before);
  });
});
