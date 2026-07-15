import type { Element, Root, Text } from "hast";
import { SKIP, visit } from "unist-util-visit";

// 跳过这些标签内的文本（代码、链接等已经在结构上突出；mark 是本插件生成的，避免递归）
const SKIP_TAGS = new Set(["code", "pre", "kbd", "script", "style", "a", "mark"]);

interface MarkElement extends Element {
  tagName: "mark";
}

/** 构造一个 <mark> 元素包裹匹配文本 */
function createMarkElement(text: string): MarkElement {
  return {
    type: "element",
    tagName: "mark",
    properties: { className: ["search-highlight"] },
    children: [{ type: "text", value: text } as Text],
  };
}

/**
 * 在文本节点中查找所有匹配词，返回拆分后的子节点数组。
 * 匹配大小写不敏感；返回 null 表示无匹配，调用方应保留原节点。
 */
function splitTextByTerms(text: string, terms: RegExp[]): Array<Text | MarkElement> | null {
  if (terms.length === 0 || text.length === 0) return null;

  // 合并为一个全局正则：(?=term1|term2|...)
  // 用 lookahead 以便能连续匹配（避免吃掉边界字符）
  const combined = new RegExp(`(${terms.map((t) => t.source).join("|")})`, "gi");
  const result: Array<Text | MarkElement> = [];
  let lastIndex = 0;
  let match: RegExpExecArray | null;

  while ((match = combined.exec(text)) !== null) {
    const start = match.index;
    const end = start + match[0].length;

    if (start > lastIndex) {
      result.push({ type: "text", value: text.slice(lastIndex, start) } as Text);
    }
    result.push(createMarkElement(match[0]));
    lastIndex = end;

    // 防止零宽匹配导致死循环
    if (match[0].length === 0) {
      combined.lastIndex++;
    }
  }

  if (lastIndex === 0) return null; // 无匹配
  if (lastIndex < text.length) {
    result.push({ type: "text", value: text.slice(lastIndex) } as Text);
  }
  return result;
}

/** Rehype 插件：在文本节点中高亮搜索关键词，包裹为 <mark className="search-highlight"> */
export const rehypeHighlightSearch = (terms: string[]) => {
  // 预处理：转义正则特殊字符，过滤空串/过短词，构造大小写不敏感的正则
  const regexes = terms
    .map((t) => t.trim())
    .filter((t) => t.length >= 1)
    .map((t) => new RegExp(t.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), "i"));

  if (regexes.length === 0) {
    return () => () => {}; // 无关键词，无操作
  }

  return (tree: Root) => {
    visit(tree, "text", (node: Text, index, parent) => {
      if (!parent || index == null) return;
      // 跳过代码/链接等结构内的文本
      if (parent.type === "element" && SKIP_TAGS.has((parent as Element).tagName)) return;

      const replaced = splitTextByTerms(node.value, regexes);
      if (replaced && replaced.length > 0) {
        parent.children.splice(index, 1, ...replaced);
        // 跳过新插入的节点（含 mark 元素），避免递归重复处理
        return [SKIP, index + replaced.length];
      }
    });
  };
};
