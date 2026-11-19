/**
 * VeinMind 前端控制器
 *
 * 负责：
 *  - 通过 tauri.invoke 调用后端命令
 *  - Canvas 渲染思维导图（读取后端计算的布局坐标）
 *  - 键盘/鼠标交互（拖拽平移、滚轮缩放、双击编辑、快捷键）
 *
 * 设计要点（与后端呼应）：
 *  - View 层只持有“渲染快照”，不维护业务状态，所有变更走 invoke。
 *  - 使用 Observer 思想：listen 监听后端事件 → 触发重绘。
 */

const { invoke } = window.__TAURI__.tauri;

// ---------- 全局视图状态 ----------
const canvas = document.getElementById('mindmap-canvas');
const ctx = canvas.getContext('2d');
const statusText = document.getElementById('status-text');
const docNameEl = document.getElementById('doc-name');

let view = {
  document: null,       // { mind_map: MindMap, layout: {node_id: {x,y}} }
  selectedId: null,
  pan: { x: 0, y: 0 },
  zoom: 1,
  isDragging: false,
  dragStart: { x: 0, y: 0 },
  panStart: { x: 0, y: 0 },
  nodeDrag: null,  // { nodeId, startMouse, startPos, hoverTargetId }
};

// ---------- Canvas 尺寸自适应 ----------
function resizeCanvas() {
  const dpr = window.devicePixelRatio || 1;
  const rect = canvas.getBoundingClientRect();
  canvas.width = rect.width * dpr;
  canvas.height = rect.height * dpr;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  render();
}
window.addEventListener('resize', resizeCanvas);

// ---------- 渲染 ----------
function render() {
  const rect = canvas.getBoundingClientRect();
  ctx.fillStyle = getCss('--canvas-bg');
  ctx.fillRect(0, 0, rect.width, rect.height);

  if (!view.document || !view.document.layout) return;

  const map = view.document.mind_map;
  const layout = view.document.layout;

  ctx.save();
  ctx.translate(view.pan.x, view.pan.y);
  ctx.scale(view.zoom, view.zoom);

  // 画连线（父→子）
  for (const node of map.nodes) {
    const parent = node.parent_id ? map.nodes.find(n => n.id === node.parent_id) : null;
    if (!parent) continue;
    if (parent.folded) continue; // 折叠则不画子树

    const p = layout[parent.id];
    const c = layout[node.id];
    if (!p || !c) continue;

    ctx.beginPath();
    ctx.moveTo(p.x, p.y);
    // 贝塞尔曲线让连线更柔和
    const midX = (p.x + c.x) / 2;
    ctx.bezierCurveTo(midX, p.y, midX, c.y, c.x, c.y);
    ctx.strokeStyle = getCss('--line-color');
    ctx.lineWidth = 1.5;
    ctx.stroke();
  }

  // 画关系线（跨节点自定义连接，Edge 实体）
  for (const edge of (map.edges || [])) {
    const p = layout[edge.source];
    const c = layout[edge.target];
    if (!p || !c) continue;
    // 用虚线 + 强调色区分于父子连线
    ctx.save();
    ctx.beginPath();
    ctx.moveTo(p.x, p.y);
    const midX = (p.x + c.x) / 2;
    ctx.bezierCurveTo(midX, p.y, midX, c.y, c.x, c.y);
    ctx.strokeStyle = getCss('--error');
    ctx.lineWidth = 2;
    ctx.setLineDash([8, 4]);
    ctx.stroke();
    ctx.restore();
  }

  // 画节点
  for (const node of map.nodes) {
    const pos = layout[node.id];
    if (!pos) continue;

    const isRoot = node.id === map.root_id;
    const isSelected = node.id === view.selectedId;
    const size = measureNode(node.text, isRoot, node.style);

    // 节点矩形（根节点用主色，其余用默认面或自定义背景色）
    ctx.fillStyle = node.style.bg_color || (isRoot ? getCss('--primary') : getCss('--node-default'));
    roundRect(ctx, pos.x - size.w/2, pos.y - size.h/2, size.w, size.h, 8);
    ctx.fill();

    // 选中边框：主色 + 光晕
    if (isSelected) {
      ctx.strokeStyle = getCss('--primary');
      ctx.lineWidth = 2.5;
      ctx.stroke();
    }

    // 拖拽悬停高亮：目标节点画虚线强调框（虚线 + 外扩 4px）
    if (view.nodeDrag && view.nodeDrag.hoverTargetId === node.id) {
      ctx.save();
      ctx.strokeStyle = getCss('--success');
      ctx.lineWidth = 2.5;
      ctx.setLineDash([6, 4]);
      roundRect(ctx, pos.x - size.w/2 - 4, pos.y - size.h/2 - 4, size.w + 8, size.h + 8, 8);
      ctx.stroke();
      ctx.restore();
    }

    // 搜索匹配高亮：匹配节点画虚线框；当前匹配项加粗
    if (view.search && view.search.matches.includes(node.id)) {
      const isCurrent = view.search.matches[view.search.currentIndex] === node.id;
      ctx.save();
      ctx.strokeStyle = isCurrent ? getCss('--error') : getCss('--primary');
      ctx.lineWidth = isCurrent ? 3 : 2;
      ctx.setLineDash(isCurrent ? [] : [5, 4]);
      roundRect(ctx, pos.x - size.w/2 - 3, pos.y - size.h/2 - 3, size.w + 6, size.h + 6, 8);
      ctx.stroke();
      ctx.restore();
    }

    // 折叠标记
    if (node.folded) {
      ctx.fillStyle = getCss('--text-muted');
      ctx.font = '12px sans-serif';
      ctx.fillText('▸', pos.x + size.w/2 + 4, pos.y + 4);
    }

    // 文本 + 图标（水平排列：图标在左、文本在右，整体以节点中心对称，不重叠）
    ctx.fillStyle = node.style.fg_color || (isRoot ? getCss('--text-inverse') : getCss('--node-text'));
    const weight = node.style.bold ? 'bold' : (isRoot ? 'bold' : 'normal');
    const italic = node.style.italic ? 'italic' : '';
    const sizePx = node.style.font_size || (isRoot ? 16 : 14);
    ctx.font = `${italic} ${weight} ${sizePx}px sans-serif`.trim();
    ctx.textBaseline = 'middle';
    const plain = stripMarkdown(node.text);
    const lines = plain.split('\n').filter(l => l.length > 0);
    const icon = node.style.icon || '';
    const gap = 6;
    // 图标宽度（emoji 用 measureText 测量，兜底 18px）
    let iconW = 0;
    if (icon) {
      ctx.font = `${italic} ${sizePx}px sans-serif`.trim();
      iconW = ctx.measureText(icon).width || 18;
      ctx.font = `${italic} ${weight} ${sizePx}px sans-serif`.trim();
    }
    // 文本最长行宽度
    let textW = 0;
    for (const line of lines) {
      const w = ctx.measureText(line).width;
      if (w > textW) textW = w;
    }
    // 内容整体以节点中心对称：左起点 = pos.x - 总宽/2
    const totalW = iconW + gap + textW;
    const contentX = pos.x - totalW / 2;
    ctx.textAlign = 'left';
    if (icon) {
      ctx.fillText(icon, contentX, pos.y);
    }
    const textLeft = contentX + iconW + gap;
    // 文本装饰线：下划线 / 删除线（按每行实际宽度绘制）
    const drawDecorations = (x, y, text) => {
      const tw = ctx.measureText(text).width;
      if (node.style.underline) {
        ctx.beginPath();
        ctx.moveTo(x, y + 3);
        ctx.lineTo(x + tw, y + 3);
        ctx.stroke();
      }
      if (node.style.strikethrough) {
        ctx.beginPath();
        ctx.moveTo(x, y - 1);
        ctx.lineTo(x + tw, y - 1);
        ctx.stroke();
      }
    };
    if (lines.length <= 1) {
      ctx.fillText(plain, textLeft, pos.y);
      drawDecorations(textLeft, pos.y, plain);
    } else {
      const lineH = isRoot ? 20 : 16;
      const startY = pos.y - ((lines.length - 1) * lineH) / 2;
      lines.forEach((line, idx) => {
        const ly = startY + idx * lineH;
        ctx.fillText(line, textLeft, ly);
        drawDecorations(textLeft, ly, line);
      });
    }
    ctx.textAlign = 'center';
  }

  ctx.restore();

  // 小地图 / 鸟瞰图：独立组件（#minimap-canvas，右下角 180×120）
  drawMiniMap();
}

/** 小地图主题配色映射（与 prototype.html 一致） */
const THEME_MINIMAP = {
  light:    { bg: '#F8F8FB', line: '#D0D0DC', primary: '#5B5BD6', leaf: '#9CA3AF' },
  dark:     { bg: '#1A1A28', line: '#3A3A52', primary: '#7B7BE8', leaf: '#A0A0B8' },
  ocean:    { bg: '#EBF3F5', line: '#C0D4D9', primary: '#0E8A96', leaf: '#9CA3AF' },
  amber:    { bg: '#F6EFE5', line: '#DECBB0', primary: '#C2600A', leaf: '#A08B6E' },
  forest:   { bg: '#17221C', line: '#33493C', primary: '#4ADE80', leaf: '#A0A0B8' },
  midnight: { bg: '#1C162C', line: '#3E3158', primary: '#A78BFA', leaf: '#A0A0B8' },
};

/**
 * 计算小地图布局映射（整图包围盒 → 小地图像素坐标），绘制与导航共用。
 * @returns {null | {w,h,minX,minY,bw,bh,scale,ox,oy,toWorld}}
 */
function getMiniMapTransform() {
  const mmCanvas = document.getElementById('minimap-canvas');
  if (!mmCanvas || !view.document || !view.document.layout) return null;
  const layout = view.document.layout;
  const ids = Object.keys(layout);
  if (ids.length === 0) return null;
  const w = mmCanvas.width;
  const h = mmCanvas.height;
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
  for (const id of ids) {
    const p = layout[id];
    if (p.x < minX) minX = p.x;
    if (p.y < minY) minY = p.y;
    if (p.x > maxX) maxX = p.x;
    if (p.y > maxY) maxY = p.y;
  }
  const bw = Math.max(maxX - minX, 1);
  const bh = Math.max(maxY - minY, 1);
  const scale = Math.min(w / bw, h / bh);
  const ox = (w - bw * scale) / 2;
  const oy = (h - bh * scale) / 2;
  return {
    w, h, minX, minY, bw, bh, scale, ox, oy,
    /** 小地图像素坐标 → 世界坐标 */
    toWorld: (mx, my) => ({
      x: minX + (mx - ox) / scale,
      y: minY + (my - oy) / scale,
    }),
  };
}

/**
 * 绘制小地图（独立 #minimap-canvas 组件）：
 * 整图布局缩略图 + 视口框，配色随主题联动。
 */
function drawMiniMap() {
  const mmCanvas = document.getElementById('minimap-canvas');
  if (!mmCanvas) return;
  const mctx = mmCanvas.getContext('2d');
  // 高分屏适配：物理尺寸 = CSS 尺寸 × devicePixelRatio，避免放大后模糊
  const dpr = window.devicePixelRatio || 1;
  const cssRect = mmCanvas.getBoundingClientRect();
  const pw = Math.max(1, Math.round(cssRect.width * dpr));
  const ph = Math.max(1, Math.round(cssRect.height * dpr));
  if (mmCanvas.width !== pw || mmCanvas.height !== ph) {
    mmCanvas.width = pw;
    mmCanvas.height = ph;
  }
  const w = mmCanvas.width;
  const h = mmCanvas.height;
  const meta = THEME_MINIMAP[getTheme()] || THEME_MINIMAP.light;

  // 背景
  mctx.clearRect(0, 0, w, h);
  mctx.fillStyle = meta.bg;
  mctx.fillRect(0, 0, w, h);

  const tf = getMiniMapTransform();
  if (!tf) return;
  const map = view.document.mind_map;
  const layout = view.document.layout;
  const ids = Object.keys(layout);
  const { minX, minY, scale, ox, oy } = tf;

  // 连接线
  mctx.strokeStyle = meta.line;
  mctx.lineWidth = 0.5;
  for (const node of map.nodes) {
    const parent = node.parent_id ? map.nodes.find(n => n.id === node.parent_id) : null;
    if (!parent) continue;
    const p = layout[parent.id];
    const c = layout[node.id];
    if (!p || !c) continue;
    mctx.beginPath();
    mctx.moveTo(ox + (p.x - minX) * scale, oy + (p.y - minY) * scale);
    mctx.lineTo(ox + (c.x - minX) * scale, oy + (c.y - minY) * scale);
    mctx.stroke();
  }

  // 节点点（根节点用主色）
  for (const id of ids) {
    const p = layout[id];
    const isRoot = map.root_id === id;
    mctx.beginPath();
    mctx.arc(ox + (p.x - minX) * scale, oy + (p.y - minY) * scale, isRoot ? 3 : 2, 0, Math.PI * 2);
    mctx.fillStyle = isRoot ? meta.primary : meta.leaf;
    mctx.fill();
  }

  // 视口框（虚线）：裁剪到小地图画布内，保证始终可见
  // （视口大于整图时框铺满画布边界；视口部分越界时只显示重叠段）
  const rect = canvas.getBoundingClientRect();
  const vpLeft = -view.pan.x / view.zoom;
  const vpTop = -view.pan.y / view.zoom;
  const vpRight = vpLeft + rect.width / view.zoom;
  const vpBottom = vpTop + rect.height / view.zoom;
  const MIN_MM_SIZE = 4; // 视口框最小可见尺寸
  const rx = ox + (vpLeft - minX) * scale;
  const ry = oy + (vpTop - minY) * scale;
  const rw = Math.max((vpRight - vpLeft) * scale, MIN_MM_SIZE);
  const rh = Math.max((vpBottom - vpTop) * scale, MIN_MM_SIZE);
  const x1 = Math.max(0, Math.min(rx, w - MIN_MM_SIZE));
  const y1 = Math.max(0, Math.min(ry, h - MIN_MM_SIZE));
  const x2 = Math.min(w, Math.max(rx + rw, MIN_MM_SIZE));
  const y2 = Math.min(h, Math.max(ry + rh, MIN_MM_SIZE));
  mctx.strokeStyle = meta.primary;
  mctx.lineWidth = 1;
  mctx.setLineDash([3, 2]);
  mctx.strokeRect(x1, y1, x2 - x1, y2 - y1);
  mctx.setLineDash([]);
}

// ---------- 小地图导航：点击跳转视口中心，拖拽平移视口 ----------
const mmCanvas = document.getElementById('minimap-canvas');
let mmDrag = null; // { startClientX, startClientY, panStart, startWorld }

/** 鼠标事件 → 小地图内部像素坐标（canvas 尺寸与 CSS 显示尺寸可能不同） */
function mmMousePos(e) {
  const rect = mmCanvas.getBoundingClientRect();
  return {
    mx: (e.clientX - rect.left) * (mmCanvas.width / rect.width),
    my: (e.clientY - rect.top) * (mmCanvas.height / rect.height),
  };
}

mmCanvas.addEventListener('mousedown', (e) => {
  const tf = getMiniMapTransform();
  if (!tf) return;
  e.preventDefault();
  e.stopPropagation();
  const { mx, my } = mmMousePos(e);
  const world = tf.toWorld(mx, my);
  // 点击：视口中心跳到该世界坐标
  const mainRect = canvas.getBoundingClientRect();
  view.pan.x = mainRect.width / 2 - world.x * view.zoom;
  view.pan.y = mainRect.height / 2 - world.y * view.zoom;
  render();
  // 拖拽：记录起点，按下点对应的世界坐标始终跟随鼠标
  mmDrag = {
    startClientX: e.clientX,
    startClientY: e.clientY,
    panStart: { ...view.pan },
    startWorld: world,
  };
});

window.addEventListener('mousemove', (e) => {
  if (!mmDrag) return;
  const tf = getMiniMapTransform();
  if (!tf) return;
  const { mx, my } = mmMousePos(e);
  const cur = tf.toWorld(mx, my);
  // 视口平移量 = 按下点世界坐标的位移 × zoom
  view.pan.x = mmDrag.panStart.x + (mmDrag.startWorld.x - cur.x) * view.zoom;
  view.pan.y = mmDrag.panStart.y + (mmDrag.startWorld.y - cur.y) * view.zoom;
  render();
});

window.addEventListener('mouseup', () => {
  mmDrag = null;
});

function measureNode(text, isRoot, style) {
  const plain = stripMarkdown(text);
  const lines = Math.max(plain.split('\n').filter(l => l.length > 0).length, 1);
  // 字号/加粗读取节点样式，背景尺寸随文本样式自适应
  const bold = !!(style && style.bold) || !!isRoot;
  const sizePx = (style && style.font_size) || (isRoot ? 16 : 14);
  ctx.font = `${bold ? 'bold' : 'normal'} ${sizePx}px sans-serif`;
  const metrics = ctx.measureText(plain);
  // 图标宽度（图标水平排列在文本左侧，背景应包含其宽度）
  const icon = (style && style.icon) || '';
  let iconW = 0;
  if (icon) {
    const prev = ctx.font;
    ctx.font = `${sizePx}px sans-serif`;
    iconW = ctx.measureText(icon).width || 18;
    ctx.font = prev;
  }
  const gap = icon ? 6 : 0;
  const w = Math.max(metrics.width + iconW + gap + 24, 60);
  // 行高与节点高度随字号自适应
  const lineH = sizePx + 4;
  const baseH = isRoot ? 36 : 28;
  const h = baseH + (lines - 1) * lineH + (sizePx - (isRoot ? 16 : 14)) * 1.2;
  return { w, h, lines };
}

/**
 * 剥离 Markdown 子集标记，返回纯文本。
 * 与后端 `Node::plain_text()` 行为对齐（bold/italic/link/code/heading/list）。
 */
function stripMarkdown(text) {
  return String(text || '').split('\n').map(line => {
    // 标题前缀
    line = line.replace(/^#+\s*/, '');
    // 列表标记
    line = line.replace(/^[-*]\s+/, '');
    // 行内：code、bold、italic、link
    line = line.replace(/`([^`]*)`/g, '$1');
    line = line.replace(/\*\*([^*]*)\*\*/g, '$1');
    line = line.replace(/\*([^*]*)\*/g, '$1');
    line = line.replace(/\[([^\]]*)\]\([^)]*\)/g, '$1');
    return line;
  }).join('\n');
}

function roundRect(ctx, x, y, w, h, r) {
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.arcTo(x + w, y, x + w, y + h, r);
  ctx.arcTo(x + w, y + h, x, y + h, r);
  ctx.arcTo(x, y + h, x, y, r);
  ctx.arcTo(x, y, x + w, y, r);
  ctx.closePath();
}

function getCss(varName) {
  return getComputedStyle(document.documentElement).getPropertyValue(varName).trim();
}

// ---------- 数据加载 ----------
/**
 * 将根节点（父节点/中心主题）居中到视口中心。
 * 屏幕坐标 = 世界坐标 * zoom + pan，故 pan = 视口中心 - 根坐标 * zoom。
 */
function centerOnRoot() {
  if (!view.document || !view.document.layout) return;
  const map = view.document.mind_map;
  const rootId = map.root_id;
  if (!rootId) return;
  const pos = view.document.layout[rootId];
  if (!pos) return;
  const rect = canvas.getBoundingClientRect();
  view.pan.x = rect.width / 2 - pos.x * view.zoom;
  view.pan.y = rect.height / 2 - pos.y * view.zoom;
}

async function loadDocument() {
  try {
    statusText.textContent = '加载文档…';
    view.document = await invoke('get_document');
    if (!view.selectedId && view.document.mind_map.root_id) {
      view.selectedId = view.document.mind_map.root_id;
    }
    centerOnRoot(); // 启动后根节点居中显示
    docNameEl.textContent = view.document.mind_map.title || '未命名';
    document.getElementById('doc-title-input').value = view.document.mind_map.title || '';
    statusText.textContent = '就绪';
    render();
    updateUndoRedoButtons();
    updateNodeStats();
  } catch (e) {
    statusText.textContent = '加载失败: ' + e;
    console.error(e);
  }
}

// ---------- 文档标题可编辑（多文档管理） ----------
const docTitleInput = document.getElementById('doc-title-input');

async function saveTitle() {
  const title = docTitleInput.value.trim();
  if (!title) return;
  await execCommand('update_document_title', { title });
  docNameEl.textContent = title;
}
docTitleInput.addEventListener('keydown', (e) => {
  if (e.key === 'Enter') {
    e.preventDefault();
    docTitleInput.blur(); // 触发 blur 保存
  }
});
docTitleInput.addEventListener('blur', saveTitle);

// ---------- 最近文件列表（localStorage 持久化，最多 8 条） ----------
const recentFilesEl = document.getElementById('recent-files');
const RECENT_KEY = 'veinmind.recent';

function getRecentFiles() {
  try {
    return JSON.parse(localStorage.getItem(RECENT_KEY) || '[]');
  } catch (e) {
    return [];
  }
}

function addRecentFile(path) {
  if (!path) return;
  let recents = getRecentFiles().filter(p => p !== path);
  recents.unshift(path);
  recents = recents.slice(0, 8);
  localStorage.setItem(RECENT_KEY, JSON.stringify(recents));
  refreshRecentFiles();
}

function refreshRecentFiles() {
  const recents = getRecentFiles();
  recentFilesEl.innerHTML = '<option value="">最近文件…</option>';
  for (const p of recents) {
    const opt = document.createElement('option');
    opt.value = p;
    opt.textContent = p.split('/').pop() || p;
    recentFilesEl.appendChild(opt);
  }
}

// 从最近列表加载文件
recentFilesEl.addEventListener('change', async (e) => {
  const path = e.target.value;
  e.target.value = ''; // 复位下拉
  if (!path) return;
  try {
    const { readTextFile } = window.__TAURI__.fs;
    const content = await readTextFile(path);
    view.document = await invoke('load_document', { json: content });
    await invoke('bind_doc_path', { path });
    view.selectedId = view.document.mind_map.root_id;
    centerOnRoot(); // 打开后根节点居中
    render();
    handleWarning(view.document);
    statusText.textContent = '已打开: ' + path;
  } catch (err) {
    statusText.textContent = '打开失败: ' + err;
    console.error(err);
  }
});

refreshRecentFiles();

// ---------- 命令封装（调用后端用例） ----------
async function execCommand(name, args = {}) {
  try {
    view.document = await invoke(name, args);
    render();
    updateUndoRedoButtons();
    updateNodeStats();
    handleWarning(view.document);
  } catch (e) {
    statusText.textContent = '操作失败: ' + e;
    console.error(e);
  }
}

/** 后端返回非致命警告（如自动保存失败）时用 Toast 提示 */
function handleWarning(dto) {
  if (dto && dto.warning) showToast(dto.warning);
}

function updateUndoRedoButtons() {
  const undoBtn = document.getElementById('btn-undo');
  const redoBtn = document.getElementById('btn-redo');
  // 后端 DTO 返回 can_undo / can_redo，驱动按钮禁用状态
  const canUndo = !!(view.document && view.document.can_undo);
  const canRedo = !!(view.document && view.document.can_redo);
  undoBtn.disabled = !canUndo;
  redoBtn.disabled = !canRedo;
}

// ---------- 交互事件 ----------
// 拖拽语义：
//   - mousedown 命中节点 → 进入"节点拖拽"模式，记录拖拽源 + 子树 id 列表
//   - mousedown 未命中 → 进入"画布平移"模式
//   - mousemove  节点拖拽 → 整棵子树随鼠标位移同步平移 + 高亮悬停目标
//   - mousemove  平移     → 移动 view.pan
//   - mouseup    节点拖拽 → 若悬停在另一个节点上，调用 move_node 改层级；否则持久化自由位置

/** 计算以 rootId 为根的整棵子树节点 id（含自身） */
function getSubtreeIds(map, rootId) {
  const ids = new Set([rootId]);
  let changed = true;
  while (changed) {
    changed = false;
    for (const n of map.nodes) {
      if (n.parent_id && ids.has(n.parent_id) && !ids.has(n.id)) {
        ids.add(n.id);
        changed = true;
      }
    }
  }
  return [...ids];
}

canvas.addEventListener('mousedown', (e) => {
  const hit = hitTest(e.offsetX, e.offsetY);
  if (hit) {
    // Shift+点击另一个节点 → 建立关系线（跨节点自定义连接）
    if (e.shiftKey && view.selectedId && view.selectedId !== hit) {
      execCommand('add_edge', { source: view.selectedId, target: hit });
      return;
    }
    view.selectedId = hit;
    view.nodeDrag = {
      nodeId: hit,
      startMouse: { x: e.offsetX, y: e.offsetY },
      // 按下时的鼠标世界坐标（用于计算每帧位移）
      prevWorld: { x: (e.offsetX - view.pan.x) / view.zoom, y: (e.offsetY - view.pan.y) / view.zoom },
      // 整棵子树（含自身），拖拽时同步平移
      subtreeIds: getSubtreeIds(view.document.mind_map, hit),
      hoverTargetId: null,
    };
    render();
  } else {
    view.isDragging = true;
    view.dragStart = { x: e.offsetX, y: e.offsetY };
    view.panStart = { ...view.pan };
  }
});

canvas.addEventListener('mousemove', (e) => {
  // 画布平移
  if (view.isDragging) {
    view.pan.x = view.panStart.x + (e.offsetX - view.dragStart.x);
    view.pan.y = view.panStart.y + (e.offsetY - view.dragStart.y);
    render();
    return;
  }
  // 节点拖拽：整棵子树随鼠标位移同步平移（被拖节点吸附跟随，子节点保持相对位置）
  if (view.nodeDrag) {
    // 世界坐标 = (屏幕坐标 - pan) / zoom，与 hitTest 的反变换一致
    const wx = (e.offsetX - view.pan.x) / view.zoom;
    const wy = (e.offsetY - view.pan.y) / view.zoom;
    // 每帧位移 = 当前鼠标世界坐标 - 上一帧鼠标世界坐标
    const dx = wx - view.nodeDrag.prevWorld.x;
    const dy = wy - view.nodeDrag.prevWorld.y;
    // 整棵子树（含被拖节点）同步平移
    for (const id of view.nodeDrag.subtreeIds) {
      const p = view.document.layout[id];
      if (p) {
        view.document.layout[id] = { x: p.x + dx, y: p.y + dy };
      }
    }
    view.nodeDrag.prevWorld = { x: wx, y: wy };
    // 悬停目标 = 鼠标下另一个节点（排除自身及其后代）
    const hover = hitTest(e.offsetX, e.offsetY);
    const draggedId = view.nodeDrag.nodeId;
    const map = view.document.mind_map;
    const isDescendant = (ancestorId, targetId) => {
      let cur = map.nodes.find(n => n.id === targetId);
      while (cur && cur.parent_id) {
        if (cur.parent_id === ancestorId) return true;
        cur = map.nodes.find(n => n.id === cur.parent_id);
      }
      return false;
    };
    if (hover && hover !== draggedId && !isDescendant(draggedId, hover)) {
      view.nodeDrag.hoverTargetId = hover;
    } else {
      view.nodeDrag.hoverTargetId = null;
    }
    render();
  }
});

window.addEventListener('mouseup', (e) => {
  // 节点拖拽结束
  if (view.nodeDrag) {
    const targetId = view.nodeDrag.hoverTargetId;
    const draggedId = view.nodeDrag.nodeId;
    view.nodeDrag = null;
    if (targetId && targetId !== draggedId) {
      // 拖到另一节点上 → 改变层级（后端重算布局）
      execCommand('move_node', { nodeId: draggedId, newParentId: targetId });
    } else {
      // 无有效目标 → 持久化拖拽终点（节点吸附在鼠标当前坐标点）。
      // 调用 set_node_free_pos 写入 Node::free_pos，布局引擎重算时优先使用，
      // 任何后续操作都不会把节点拉回算法布局位置（根治"回到中心"缺陷）。
      const endPos = view.document.layout[draggedId];
      if (endPos) {
        execCommand('set_node_free_pos', { nodeId: draggedId, x: endPos.x, y: endPos.y });
      } else {
        render();
      }
    }
  }
  view.isDragging = false;
});

canvas.addEventListener('wheel', (e) => {
  e.preventDefault();
  const factor = e.deltaY > 0 ? 0.9 : 1.1;
  view.zoom = Math.max(0.2, Math.min(5, view.zoom * factor));
  updateZoomValue();
  render();
}, { passive: false });

// ---------- 双击节点内联编辑（市面工具标配：就地修改节点内容） ----------
const inlineEditor = document.getElementById('inline-editor');
let inlineNodeId = null;

/** 关闭内联编辑器；commit=true 时提交文本到后端（走 update_node_text） */
function closeInlineEditor(commit) {
  if (!inlineNodeId) return;
  const nodeId = inlineNodeId;
  const text = inlineEditor.value;
  inlineNodeId = null;
  inlineEditor.classList.add('hidden');
  if (commit) {
    execCommand('update_node_text', { nodeId, text });
  }
}

/** 打开内联编辑器：定位到节点屏幕位置，预填文本并聚焦全选 */
function openInlineEditor(nodeId) {
  const map = view.document.mind_map;
  const layout = view.document.layout;
  const node = map.nodes.find(n => n.id === nodeId);
  const pos = layout[nodeId];
  if (!node || !pos) return;

  const isRoot = node.id === map.root_id;
  const size = measureNode(node.text, isRoot, node.style);
  // 世界坐标 → 屏幕坐标（世界 * zoom + pan）
  const sx = pos.x * view.zoom + view.pan.x;
  const sy = pos.y * view.zoom + view.pan.y;
  const w = Math.max(size.w * view.zoom + 20, 80);
  const h = Math.max(size.h * view.zoom + 10, 32);

  inlineNodeId = nodeId;
  inlineEditor.value = node.text;
  inlineEditor.style.left = (sx - w / 2) + 'px';
  inlineEditor.style.top = (sy - h / 2) + 'px';
  inlineEditor.style.width = w + 'px';
  inlineEditor.style.height = h + 'px';
  inlineEditor.style.fontSize = (isRoot ? 16 : 14) * view.zoom + 'px';
  inlineEditor.classList.remove('hidden');
  inlineEditor.focus();
  inlineEditor.select();
}

canvas.addEventListener('dblclick', (e) => {
  const hit = hitTest(e.offsetX, e.offsetY);
  if (hit) {
    openInlineEditor(hit);
  }
});

inlineEditor.addEventListener('keydown', (e) => {
  if (e.key === 'Enter' && !e.shiftKey) {
    // Enter 提交（单行）；Shift+Enter 换行
    e.preventDefault();
    closeInlineEditor(true);
  } else if (e.key === 'Escape') {
    e.preventDefault();
    closeInlineEditor(false);
  }
});

inlineEditor.addEventListener('blur', () => {
  closeInlineEditor(true);
});

// ---------- 右键上下文菜单 ----------
const contextMenu = document.getElementById('context-menu');
const edgeMenuDivider = document.getElementById('edge-menu-divider');
const edgeMenuItem = document.getElementById('menu-remove-edge');

function hideContextMenu() {
  contextMenu.classList.remove('visible');
  contextMenu.dataset.nodeId = '';
  contextMenu.dataset.edgeId = '';
}

function showContextMenu() {
  contextMenu.classList.add('visible');
}

/**
 * 关系线命中检测：判断鼠标是否点在某条关系线（贝塞尔曲线）附近。
 * 用 10 个采样点近似曲线，点到任一点距离 < 阈值即命中。
 * 返回命中的边 id，否则 null。
 */
function hitTestEdge(mx, my) {
  if (!view.document || !view.document.layout) return null;
  const map = view.document.mind_map;
  const layout = view.document.layout;
  // 反变换到世界坐标
  const x = (mx - view.pan.x) / view.zoom;
  const y = (my - view.pan.y) / view.zoom;
  const THRESHOLD = 10 / view.zoom;
  for (const edge of (map.edges || [])) {
    const p = layout[edge.source];
    const c = layout[edge.target];
    if (!p || !c) continue;
    const midX = (p.x + c.x) / 2;
    for (let i = 0; i <= 10; i++) {
      const t = i / 10;
      const mt = 1 - t;
      const bx = mt * mt * p.x + 2 * mt * t * midX + t * t * c.x;
      const by = mt * mt * p.y + 2 * mt * t * midX + t * t * c.y;
      const dx = bx - x;
      const dy = by - y;
      if (dx * dx + dy * dy < THRESHOLD * THRESHOLD) {
        return edge.id;
      }
    }
  }
  return null;
}

function positionMenu(e) {
  const menuW = 180;
  const menuH = contextMenu.offsetHeight || 180;
  const x = Math.min(e.clientX, window.innerWidth - menuW - 8);
  const y = Math.min(e.clientY, window.innerHeight - menuH - 8);
  contextMenu.style.left = x + 'px';
  contextMenu.style.top = y + 'px';
}

canvas.addEventListener('contextmenu', (e) => {
  e.preventDefault();
  // 1) 优先节点命中 → 节点菜单
  const hit = hitTest(e.offsetX, e.offsetY);
  if (hit) {
    view.selectedId = hit;
    render();
    contextMenu.dataset.nodeId = hit;
    contextMenu.dataset.edgeId = '';
    edgeMenuDivider.style.display = 'none';
    edgeMenuItem.style.display = 'none';
    showContextMenu();
    positionMenu(e);
    return;
  }
  // 2) 关系线命中 → "删除关系线"菜单
  const edgeHit = hitTestEdge(e.offsetX, e.offsetY);
  if (edgeHit) {
    contextMenu.dataset.nodeId = '';
    contextMenu.dataset.edgeId = edgeHit;
    // 隐藏节点操作项，仅显示删除关系线
    for (const item of contextMenu.querySelectorAll('.menu-item')) {
      if (item.dataset.action !== 'remove-edge') item.style.display = 'none';
    }
    edgeMenuDivider.style.display = 'block';
    edgeMenuItem.style.display = 'block';
    showContextMenu();
    positionMenu(e);
    return;
  }
  // 3) 空白处 → 隐藏
  hideContextMenu();
});

// 菜单项点击动作
contextMenu.addEventListener('click', (e) => {
  const item = e.target.closest('.menu-item');
  if (!item) return;
  const action = item.dataset.action;
  const nodeId = contextMenu.dataset.nodeId;
  const edgeId = contextMenu.dataset.edgeId;
  hideContextMenu();

  // 关系线模式：删除关系线（不依赖 nodeId）
  if (action === 'remove-edge') {
    if (edgeId) execCommand('remove_edge', { edgeId });
    return;
  }
  if (!nodeId) return;

  if (action === 'add-child') {
    execCommand('add_child_node', { parentId: nodeId, text: '新节点' });
  } else if (action === 'add-sibling') {
    const node = view.document.mind_map.nodes.find(n => n.id === nodeId);
    if (node && node.parent_id) {
      execCommand('add_sibling_node', { siblingId: nodeId, text: '新节点' });
    }
  } else if (action === 'edit') {
    const node = view.document.mind_map.nodes.find(n => n.id === nodeId);
    document.getElementById('node-text').value = node.text;
    document.getElementById('node-notes').value = node.notes || '';
    openModal(nodeId);
  } else if (action === 'fold') {
    execCommand('toggle_fold', { nodeId });
  } else if (action === 'delete') {
    execCommand('remove_node', { nodeId });
  }
});

// 点击其他区域 / Escape 关闭菜单
window.addEventListener('mousedown', (e) => {
  if (!contextMenu.contains(e.target)) hideContextMenu();
});
document.addEventListener('keydown', (e) => {
  if (e.key === 'Escape') hideContextMenu();
});

function hitTest(mx, my) {
  if (!view.document || !view.document.layout) return null;
  const map = view.document.mind_map;
  const layout = view.document.layout;
  // 反变换
  const x = (mx - view.pan.x) / view.zoom;
  const y = (my - view.pan.y) / view.zoom;
  for (const node of map.nodes) {
    const pos = layout[node.id];
    if (!pos) continue;
    const size = measureNode(node.text, node.id === map.root_id, node.style);
    if (x >= pos.x - size.w/2 && x <= pos.x + size.w/2 &&
        y >= pos.y - size.h/2 && y <= pos.y + size.h/2) {
      return node.id;
    }
  }
  return null;
}

// ---------- 工具栏按钮 ----------
document.getElementById('btn-new').onclick = async () => {
  view.document = await invoke('create_document', { title: '新建思维导图' });
  view.selectedId = view.document.mind_map.root_id;
  centerOnRoot(); // 新建后根节点居中
  docNameEl.textContent = view.document.mind_map.title || '未命名';
  document.getElementById('doc-title-input').value = view.document.mind_map.title || '';
  render();
  updateUndoRedoButtons();
  updateNodeStats(); // 新建后同步左下角节点统计
  handleWarning(view.document);
};

document.getElementById('btn-save').onclick = async () => {
  const { save } = window.__TAURI__.dialog;
  const { writeTextFile } = window.__TAURI__.fs;
  const path = await save({ defaultPath: 'veinmind.json', filters: [{ name: 'JSON', extensions: ['json'] }] });
  if (!path) return;
  try {
    const json = await invoke('save_document');
    await writeTextFile(path, json);
    // 绑定磁盘路径：之后的每次编辑会自动落盘
    await invoke('bind_doc_path', { path });
    showToast('已保存（已开启自动保存）: ' + path);
  } catch (err) {
    showToast('保存失败: ' + err);
    console.error(err);
  }
};

document.getElementById('btn-load').onclick = async () => {
  const { open } = window.__TAURI__.dialog;
  const { readTextFile } = window.__TAURI__.fs;
  const path = await open({ filters: [{ name: 'JSON', extensions: ['json'] }] });
  if (!path) return;
  const content = await readTextFile(path);
  view.document = await invoke('load_document', { json: content });
  // 绑定磁盘路径：之后的每次编辑会自动落盘到原文件
  await invoke('bind_doc_path', { path });
  view.selectedId = view.document.mind_map.root_id;
  centerOnRoot(); // 打开后根节点居中
  render();
  handleWarning(view.document);
  statusText.textContent = '已加载（已开启自动保存）: ' + path;
};

document.getElementById('btn-add-child').onclick = () => addChild();
document.getElementById('btn-add-sibling').onclick = () => addSibling();
// 添加漂浮主题（游离节点）：不挂在树上，布局到主树外侧
document.getElementById('btn-add-free').onclick = async () => {
  await execCommand('add_free_node', { text: '漂浮主题' });
};
document.getElementById('btn-delete').onclick = () => {
  if (view.selectedId) execCommand('remove_node', { nodeId: view.selectedId });
};
document.getElementById('btn-undo').onclick = () => execCommand('undo');
document.getElementById('btn-redo').onclick = () => execCommand('redo');
document.getElementById('btn-export').onclick = exportDocument;

// 导入 OPML 大纲文件（Workflowy / 幕布 / MindNode 等生态互通）
document.getElementById('btn-import-opml').onclick = async () => {
  const { open } = window.__TAURI__.dialog;
  const { readTextFile } = window.__TAURI__.fs;
  const path = await open({ filters: [
    { name: 'OPML', extensions: ['opml', 'xml'] },
  ]});
  if (!path) return;
  const opml = await readTextFile(path);
  try {
    view.document = await invoke('import_opml', { opml });
    view.selectedId = view.document.mind_map.root_id;
    centerOnRoot(); // 导入后根节点居中
    render();
    handleWarning(view.document);
    statusText.textContent = '已导入 OPML: ' + path;
  } catch (e) {
    statusText.textContent = 'OPML 导入失败: ' + e;
    console.error(e);
  }
};

// 导入 FreeMind .mm 文件（XMind / MindManager / 幕布 / ProcessOn 生态互通）
document.getElementById('btn-import-mm').onclick = async () => {
  const { open } = window.__TAURI__.dialog;
  const { readTextFile } = window.__TAURI__.fs;
  const path = await open({ filters: [
    { name: 'FreeMind', extensions: ['mm'] },
  ]});
  if (!path) return;
  const mm = await readTextFile(path);
  try {
    view.document = await invoke('import_freemind', { mm });
    view.selectedId = view.document.mind_map.root_id;
    centerOnRoot(); // 导入后根节点居中
    render();
    handleWarning(view.document);
    statusText.textContent = '已导入 FreeMind: ' + path;
  } catch (e) {
    statusText.textContent = 'FreeMind 导入失败: ' + e;
    console.error(e);
  }
};

document.getElementById('layout-strategy').onchange = async (e) => {
  await invoke('set_layout_strategy', { strategy: e.target.value });
  view.document = await invoke('get_document');
  render();
};

// ---------- 节点样式控件（作用于选中节点） ----------
function selectedNode() {
  if (!view.document || !view.selectedId) return null;
  return view.document.mind_map.nodes.find(n => n.id === view.selectedId) || null;
}

/** 获取选中节点当前样式，无选中则返回 null */
function selectedStyle() {
  const node = selectedNode();
  return node ? node.style : null;
}

async function applyStyle(patch) {
  const node = selectedNode();
  if (!node) return;
  await execCommand('update_node_style', { nodeId: node.id, style: { ...node.style, ...patch } });
}

document.getElementById('btn-bold').onclick = () => {
  const style = selectedStyle();
  if (!style) return;
  const next = !style.bold;
  applyStyle({ bold: next });
  // 高亮必须用新值驱动（旧值会导致点击后按钮状态与节点实际样式脱节，感知"加粗无效"）
  document.getElementById('btn-bold').classList.toggle('active', next);
};

// 斜体 / 下划线 / 删除线（切换 + active 高亮用新值驱动）
function toggleStyleField(field, btnId) {
  const style = selectedStyle();
  if (!style) return;
  const next = !style[field];
  applyStyle({ [field]: next });
  document.getElementById(btnId).classList.toggle('active', next);
}
document.getElementById('btn-italic').onclick = () => toggleStyleField('italic', 'btn-italic');
document.getElementById('btn-underline').onclick = () => toggleStyleField('underline', 'btn-underline');
document.getElementById('btn-strike').onclick = () => toggleStyleField('strikethrough', 'btn-strike');

document.getElementById('btn-font-minus').onclick = () => {
  const style = selectedStyle();
  if (!style) return;
  const cur = style.font_size || 14;
  const next = Math.max(10, cur - 2);
  applyStyle({ font_size: next });
  document.getElementById('font-size-value').textContent = next; // 数字同步
};

document.getElementById('btn-font-plus').onclick = () => {
  const style = selectedStyle();
  if (!style) return;
  const cur = style.font_size || 14;
  const next = Math.min(28, cur + 2);
  applyStyle({ font_size: next });
  document.getElementById('font-size-value').textContent = next; // 数字同步
};

// ---------- 属性面板：色板 / 图标格（替代原 select） ----------
const NODE_COLOR_VARS = {
  red: '--node-red-bg', orange: '--node-orange-bg', green: '--node-green-bg',
  blue: '--node-blue-bg', purple: '--node-purple-bg', gray: '--node-gray-bg',
};

// 色板点击 → 应用/清除填充色
document.getElementById('color-palette').addEventListener('click', (e) => {
  const swatch = e.target.closest('.color-swatch');
  if (!swatch) return;
  const color = swatch.dataset.color;
  document.querySelectorAll('#color-palette .color-swatch').forEach(s => s.classList.remove('selected'));
  swatch.classList.add('selected');
  if (!color) {
    applyStyle({ bg_color: '' }); // 默认（空字符串 = 清除，后端 merge_style 视为 None）
  } else {
    applyStyle({ bg_color: getCss(NODE_COLOR_VARS[color]) });
  }
});

// 图标格点击 → 应用/清除图标
document.getElementById('icon-picker').addEventListener('click', (e) => {
  const opt = e.target.closest('.icon-option');
  if (!opt) return;
  const icon = opt.dataset.icon;
  document.querySelectorAll('#icon-picker .icon-option').forEach(o => o.classList.remove('selected'));
  opt.classList.add('selected');
  applyStyle({ icon: icon }); // data-icon="" 传空字符串 = 清除（后端 merge_style 视为 None）
});

// 文字颜色色板：默认 + 3 常用色 + 颜色盘（自定义颜色）
const NODE_FG_VARS = {
  red: '--node-red-fg', blue: '--node-blue-fg', green: '--node-green-fg',
};
document.getElementById('fg-palette').addEventListener('click', (e) => {
  const swatch = e.target.closest('.color-swatch');
  if (!swatch) return;
  // 颜色盘（.color-picker 内含 input[type=color]）交给 input 事件处理
  if (swatch.classList.contains('color-picker')) return;
  const color = swatch.dataset.color;
  document.querySelectorAll('#fg-palette .color-swatch').forEach(s => s.classList.remove('selected'));
  swatch.classList.add('selected');
  if (!color) {
    applyStyle({ fg_color: '' }); // 默认（空字符串 = 清除）
  } else {
    applyStyle({ fg_color: getCss(NODE_FG_VARS[color]) });
  }
});

// 颜色盘：input[type=color] 选择任意颜色 → 应用 fg_color
document.getElementById('fg-color-input').addEventListener('input', (e) => {
  const val = e.target.value;
  document.querySelectorAll('#fg-palette .color-swatch').forEach(s => s.classList.remove('selected'));
  e.target.closest('.color-swatch').classList.add('selected');
  applyStyle({ fg_color: val });
});

async function addChild() {
  if (!view.selectedId) return;
  await execCommand('add_child_node', { parentId: view.selectedId, text: '新节点' });
}
async function addSibling() {
  if (!view.selectedId) return;
  const node = view.document.mind_map.nodes.find(n => n.id === view.selectedId);
  if (!node || !node.parent_id) return;
  await execCommand('add_sibling_node', { siblingId: view.selectedId, text: '新节点' });
}

/** 把 dataURL（如 canvas.toDataURL 输出）转换为 Uint8Array 供 writeBinaryFile 写入 */
function dataUrlToBytes(dataUrl) {
  const base64 = dataUrl.split(',')[1];
  const bin = atob(base64);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) {
    bytes[i] = bin.charCodeAt(i);
  }
  return bytes;
}

async function exportDocument() {
  const { save } = window.__TAURI__.dialog;
  const { writeTextFile, writeBinaryFile } = window.__TAURI__.fs;
  const path = await save({ defaultPath: 'veinmind.svg', filters: [
    { name: 'PNG 图片', extensions: ['png'] },
    { name: 'SVG', extensions: ['svg'] },
    { name: 'Markdown', extensions: ['md'] },
    { name: 'FreeMind', extensions: ['mm'] },
    { name: 'OPML', extensions: ['opml'] },
  ]});
  if (!path) return;
  // PNG：直接把当前 Canvas 视图导出为图片（所见即所得）
  if (path.endsWith('.png')) {
    const dataUrl = canvas.toDataURL('image/png');
    await writeBinaryFile(path, dataUrlToBytes(dataUrl));
    statusText.textContent = '已导出 PNG: ' + path;
    return;
  }
  const format = path.endsWith('.md') ? 'markdown'
    : path.endsWith('.mm') ? 'freemind'
    : path.endsWith('.opml') ? 'opml'
    : 'svg';
  const content = await invoke('export_document', { format });
  await writeTextFile(path, content);
  statusText.textContent = '已导出: ' + path;
}

// ---------- 编辑模态框（backdrop + visible 结构） ----------
function openModal(nodeId) {
  const modal = document.getElementById('editor-modal');
  modal.dataset.nodeId = nodeId;
  document.getElementById('modal-backdrop').classList.add('visible');
  modal.classList.add('visible');
}
function closeModal() {
  document.getElementById('modal-backdrop').classList.remove('visible');
  document.getElementById('editor-modal').classList.remove('visible');
}
document.getElementById('btn-modal-close').onclick = closeModal;
document.getElementById('modal-backdrop').addEventListener('click', closeModal);
document.getElementById('btn-cancel-edit').onclick = closeModal;
document.getElementById('btn-confirm-edit').onclick = async () => {
  const modal = document.getElementById('editor-modal');
  const nodeId = modal.dataset.nodeId;
  const text = document.getElementById('node-text').value;
  const notes = document.getElementById('node-notes').value;
  closeModal();
  await execCommand('update_node_text', { nodeId, text });
  // 备注独立保存（不影响撤销/重做的文本历史）
  await execCommand('update_node_notes', { nodeId, notes });
};

// ---------- 搜索 / 跳转 ----------
const searchInput = document.getElementById('search-input');
const searchCount = document.getElementById('search-count');
// 搜索状态：query 匹配文本/备注，matches 为节点 id 列表，currentIndex 为当前项
view.search = { query: '', matches: [], currentIndex: -1 };

function runSearch() {
  const q = searchInput.value.trim().toLowerCase();
  const map = view.document ? view.document.mind_map : null;
  view.search.query = q;
  if (!q || !map) {
    view.search.matches = [];
    view.search.currentIndex = -1;
  } else {
    view.search.matches = map.nodes
      .filter(n => (n.text + ' ' + (n.notes || '')).toLowerCase().includes(q))
      .map(n => n.id);
    view.search.currentIndex = view.search.matches.length > 0 ? 0 : -1;
  }
  updateSearchCount();
  render();
  if (view.search.currentIndex >= 0) jumpToCurrent();
}

function updateSearchCount() {
  const total = view.search.matches.length;
  const idx = view.search.currentIndex >= 0 ? view.search.currentIndex + 1 : 0;
  searchCount.textContent = total > 0 ? `${idx}/${total}` : '';
}

/** 将当前匹配节点居中到视口 */
function jumpToCurrent() {
  const id = view.search.matches[view.search.currentIndex];
  if (!id || !view.document || !view.document.layout) return;
  const pos = view.document.layout[id];
  if (!pos) return;
  const rect = canvas.getBoundingClientRect();
  view.pan.x = rect.width / 2 - pos.x * view.zoom;
  view.pan.y = rect.height / 2 - pos.y * view.zoom;
  render();
}

// 输入时实时过滤
searchInput.addEventListener('input', runSearch);

// Enter 循环跳转下一个匹配；Escape 清空关闭
searchInput.addEventListener('keydown', (e) => {
  if (e.key === 'Enter') {
    e.preventDefault();
    if (view.search.matches.length === 0) return;
    view.search.currentIndex = (view.search.currentIndex + 1) % view.search.matches.length;
    updateSearchCount();
    jumpToCurrent();
  } else if (e.key === 'Escape') {
    e.preventDefault();
    searchInput.value = '';
    runSearch();
    searchInput.blur();
  }
});

// ---------- 属性面板（选中节点滑入） ----------
const propsPanel = document.getElementById('props-panel');
document.getElementById('props-close').addEventListener('click', () => {
  propsPanel.classList.remove('visible');
});
// 选中节点时显示属性面板 + 同步字号显示
canvas.addEventListener('mousedown', (e) => {
  const hit = hitTest(e.offsetX, e.offsetY);
  if (hit) {
    propsPanel.classList.add('visible');
    // 面板打开时字号数字反映选中节点实际字号
    const node = view.document.mind_map.nodes.find(n => n.id === hit);
    if (node) {
      document.getElementById('font-size-value').textContent = node.style.font_size || 14;
    }
  }
}, true);

// ---------- 搜索浮栏切换（画布顶部胶囊） ----------
const searchBar = document.getElementById('search-bar');
function toggleSearchBar(show) {
  const isVisible = searchBar.style.display !== 'none';
  const next = (show !== undefined) ? show : !isVisible;
  searchBar.style.display = next ? 'flex' : 'none';
  if (next) { searchInput.focus(); searchInput.select(); }
}
document.getElementById('btn-search').addEventListener('click', () => toggleSearchBar());

// ---------- 缩放控制（底栏） ----------
const zoomValueEl = document.getElementById('zoom-value');
function updateZoomValue() {
  zoomValueEl.textContent = Math.round(view.zoom * 100) + '%';
}
document.getElementById('zoom-in').addEventListener('click', () => {
  view.zoom = Math.max(0.2, Math.min(5, view.zoom * 1.1));
  updateZoomValue(); render();
});
document.getElementById('zoom-out').addEventListener('click', () => {
  view.zoom = Math.max(0.2, Math.min(5, view.zoom * 0.9));
  updateZoomValue(); render();
});

// ---------- 视图控制（居中视图 / 适应屏幕） ----------
document.getElementById('btn-center').addEventListener('click', () => { centerOnRoot(); render(); });
document.getElementById('btn-fit').addEventListener('click', () => {
  if (!view.document || !view.document.layout) return;
  const layout = view.document.layout;
  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
  for (const id of Object.keys(layout)) {
    const p = layout[id];
    if (p.x < minX) minX = p.x; if (p.y < minY) minY = p.y;
    if (p.x > maxX) maxX = p.x; if (p.y > maxY) maxY = p.y;
  }
  const rect = canvas.getBoundingClientRect();
  const pad = 60;
  const zoom = Math.min((rect.width - pad * 2) / Math.max(maxX - minX, 1),
                        (rect.height - pad * 2) / Math.max(maxY - minY, 1));
  view.zoom = Math.max(0.2, Math.min(2, zoom));
  view.pan.x = rect.width / 2 - ((minX + maxX) / 2) * view.zoom;
  view.pan.y = rect.height / 2 - ((minY + maxY) / 2) * view.zoom;
  updateZoomValue(); render();
});

// ---------- Toast 通知 ----------
const toastEl = document.getElementById('toast');
let toastTimer = null;
function showToast(message, ms = 3000) {
  document.getElementById('toast-message').textContent = message;
  toastEl.classList.add('visible');
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => toastEl.classList.remove('visible'), ms);
}
// 用 Toast 接管状态栏反馈（保存/导入/导出等）
const _origStatus = statusText.textContent;

// ---------- 节点统计（底栏） ----------
const nodeCountEl = document.getElementById('node-count');
function updateNodeStats() {
  if (view.document) {
    nodeCountEl.textContent = view.document.mind_map.nodes.length + ' 个节点';
  }
}

// ---------- 主题系统（6 款主题 + 选择器 + localStorage 持久化） ----------
const LIGHT_THEMES = ['light', 'ocean', 'amber'];
const DARK_THEMES = ['dark', 'forest', 'midnight'];
const THEME_META = {
  light: { name: '经典亮色', mode: 'Light' },
  dark: { name: '经典暗色', mode: 'Dark' },
  ocean: { name: '海盐青', mode: 'Light' },
  amber: { name: '暖阳琥珀', mode: 'Light' },
  forest: { name: '墨绿森林', mode: 'Dark' },
  midnight: { name: '午夜紫', mode: 'Dark' },
};
function getTheme() {
  return document.documentElement.getAttribute('data-theme') || 'light';
}
function setTheme(name) {
  if (!THEME_META[name]) name = 'light';
  document.documentElement.setAttribute('data-theme', name);
  localStorage.setItem('veinmind-theme', name);
  document.querySelectorAll('.theme-card').forEach(card => {
    card.classList.toggle('active', card.dataset.theme === name);
  });
  drawMiniMap();
  render(); // 主题变量已随 data-theme 变化，重绘画布使节点/连线/背景同步新主题
}
// 快捷明暗切换：亮色家族 → 经典亮色；暗色家族 → 经典暗色
document.getElementById('theme-toggle').addEventListener('click', (e) => {
  e.stopPropagation();
  const isDark = DARK_THEMES.includes(getTheme());
  setTheme(isDark ? 'light' : 'dark');
});
// 主题选择器（调色板按钮 + 弹出面板）
const pickerBtn = document.getElementById('theme-picker-btn');
const themePopover = document.getElementById('theme-popover');
pickerBtn.addEventListener('click', (e) => {
  e.stopPropagation();
  const willShow = !themePopover.classList.contains('visible');
  themePopover.classList.toggle('visible', willShow);
  pickerBtn.classList.toggle('active', willShow);
});
document.addEventListener('click', () => {
  themePopover.classList.remove('visible');
  pickerBtn.classList.remove('active');
});
themePopover.addEventListener('click', (e) => e.stopPropagation());
document.querySelectorAll('.theme-card').forEach(card => {
  const activate = () => {
    setTheme(card.dataset.theme);
    themePopover.classList.remove('visible');
    pickerBtn.classList.remove('active');
  };
  card.addEventListener('click', activate);
  card.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); activate(); }
  });
});
// 启动时恢复上次主题
setTheme(localStorage.getItem('veinmind-theme') || getTheme());

// ---------- 键盘快捷键 ----------
document.addEventListener('keydown', async (e) => {
  // 编辑框中不触发快捷键
  if (e.target.tagName === 'TEXTAREA' || e.target.tagName === 'INPUT') return;

  if (e.key === 'Tab') { e.preventDefault(); await addChild(); }
  else if (e.key === 'Enter') { e.preventDefault(); await addChild(); }
  else if (e.key === 'Delete' || e.key === 'Backspace') {
    if (view.selectedId) { e.preventDefault(); await execCommand('remove_node', { nodeId: view.selectedId }); }
  }
  else if (e.key === ' ') {
    if (view.selectedId) { e.preventDefault(); await execCommand('toggle_fold', { nodeId: view.selectedId }); }
  }
  else if ((e.ctrlKey || e.metaKey) && e.key === 'z') { e.preventDefault(); await execCommand('undo'); }
  else if ((e.ctrlKey || e.metaKey) && (e.key === 'y' || (e.shiftKey && e.key === 'Z'))) { e.preventDefault(); await execCommand('redo'); }
  else if ((e.ctrlKey || e.metaKey) && e.key === 's') { e.preventDefault(); document.getElementById('btn-save').click(); }
  else if ((e.ctrlKey || e.metaKey) && e.key === 'o') { e.preventDefault(); document.getElementById('btn-load').click(); }
  else if ((e.ctrlKey || e.metaKey) && e.key === 'f') {
    // Ctrl+F 切换搜索浮栏（画布顶部胶囊）
    e.preventDefault();
    toggleSearchBar();
  }
});

// ---------- 启动 ----------
resizeCanvas();
loadDocument();
