import { parse as parseYaml } from 'yaml';
// Vite ?raw 导入: 将 YAML 文件作为字符串读取
import zhRaw from './locales/zh.yml?raw';
import enRaw from './locales/en.yml?raw';

export type Lang = 'zh' | 'en';

const dict = {
  zh: parseYaml(zhRaw) as Record<string, string>,
  en: parseYaml(enRaw) as Record<string, string>,
};

export type DictKey = string;

export function t(lang: Lang, key: DictKey): string {
  return dict[lang][key] ?? key;
}

export function tAll(lang: Lang): Record<string, string> {
  return dict[lang];
}
