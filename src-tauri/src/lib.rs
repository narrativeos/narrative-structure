pub mod project_manager;
pub mod db_engine;
pub mod markdown_parser;
pub mod page_mapper;
pub mod ocr_adapter;
pub mod mcp;
pub mod pdf_render;

use project_manager::ProjectState;
use tauri::http::{Request, Response, StatusCode};

// 内嵌 PDF.js（离线可用）
const PDF_JS: &str = include_str!("../assets/pdf.min.js");
const PDF_WORKER_JS: &str = include_str!("../assets/pdf.worker.min.js");

/// 简单 base64 编码
fn base64_encode(bytes: &[u8]) -> String {
    let chars = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    let padding = 3 - (bytes.len() % 3);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).map_or(0, |b| *b as u32);
        let b2 = chunk.get(2).map_or(0, |b| *b as u32);
        let n = (b0 << 16) | (b1 << 8) | b2;
        result.push(chars[((n >> 18) & 0x3F) as usize] as char);
        result.push(chars[((n >> 12) & 0x3F) as usize] as char);
        result.push(chars[((n >> 6) & 0x3F) as usize] as char);
        result.push(chars[(n & 0x3F) as usize] as char);
    }
    for _ in 0..padding {
        result.pop();
        result.push('=');
    }
    result
}

/// 自定义协议：narrativestructure://localhost/<file_path> 直接提供文件
fn asset_protocol(
    _ctx: tauri::UriSchemeContext<'_, tauri::Wry>,
    req: Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    use std::fs;
    use std::io::Read;

    let uri = req.uri().to_string();
    let path_str = req.uri().path().trim_start_matches('/');
    let decoded = percent_encoding::percent_decode_str(path_str).decode_utf8_lossy();
    let path = std::path::PathBuf::from(decoded.as_ref());

    // 如果是 .pdf 请求，返回内嵌 PDF.js 连续查看器
    // 使用解码后的路径判断扩展名（避免 URL 编码导致匹配失败）
    let is_pdf = path.extension().map(|e| e == "pdf").unwrap_or(false);
    let has_raw = uri.contains("raw=1");
    if is_pdf && !has_raw {
        // 读取 PDF 文件内容为 base64
        let pdf_b64 = if let Ok(mut file) = fs::File::open(&path) {
            let mut buf = Vec::new();
            if file.read_to_end(&mut buf).is_ok() {
                base64_encode(&buf)
            } else {
                String::new()
            }
        } else {
            String::new()
        };
        eprintln!("[asset_protocol] → returning HTML wrapper (size will be ~{} bytes)", PDF_JS.len() + pdf_b64.len() / 4 + 15000);
        let html = format!(r#"<!DOCTYPE html><html><head><meta charset="utf-8"><style>
body{{margin:0;background:#525659;overflow:hidden}}
#toolbar{{position:fixed;top:4px;left:8px;z-index:100;display:flex;gap:6px;align-items:center}}
#toolbar button{{background:rgba(0,0,0,0.55);color:#ccc;border:1px solid rgba(255,255,255,0.15);border-radius:3px;padding:2px 8px;font-size:11px;cursor:pointer}}
#toolbar button.active{{background:rgba(59,130,246,0.6);color:#fff;border-color:rgba(59,130,246,0.8)}}
#stage{{position:fixed;top:0;left:0;right:0;bottom:0;display:flex;flex-direction:column;align-items:center;justify-content:center}}
#prev-area{{flex:1;width:100%;display:flex;align-items:flex-end;justify-content:center;overflow:hidden}}
#curr-area{{flex:0 0 auto;width:100%;display:flex;align-items:center;justify-content:center}}
#next-area{{flex:1;width:100%;display:flex;align-items:flex-start;justify-content:center;overflow:hidden}}
.page-wrap{{position:relative;display:block;width:100%;box-sizing:border-box;box-shadow:0 2px 8px rgba(0,0,0,0.3)}}
.page-wrap canvas{{display:block}}
.page-wrap .overlay{{position:absolute;top:0;left:0;pointer-events:none}}
.page-num{{position:absolute;top:5px;right:8px;background:rgba(0,0,0,0.55);color:#ccc;padding:1px 6px;border-radius:3px;font-size:10px;font-family:monospace;pointer-events:none;z-index:5;user-select:none}}
#indicator{{position:fixed;top:4px;right:8px;background:rgba(0,0,0,0.6);color:#ccc;padding:2px 8px;border-radius:3px;font-size:11px;z-index:10}}
#nav-btns{{position:fixed;right:8px;top:50%;transform:translateY(-50%);display:flex;flex-direction:column;gap:4px;z-index:100}}
#nav-btns button{{background:rgba(0,0,0,0.55);color:#ccc;border:1px solid rgba(255,255,255,0.15);border-radius:3px;padding:8px 6px;font-size:14px;cursor:pointer}}
#nav-btns button:hover{{background:rgba(59,130,246,0.6)}}
.leg-dot{{display:inline-block;width:7px;height:7px;border-radius:2px;margin:0 2px 0 5px;vertical-align:middle}}
</style></head><body>
<div id="indicator">1 / ?</div>
<div id="toolbar" style="display:flex;align-items:center;gap:4px">
  <button id="btn-overlay" class="active" onclick="toggleOverlay()" title="显示/隐藏信息层">👁</button>
  <span style="font-size:10px;color:#999;white-space:nowrap">
    <span class="leg-dot" style="background:#ef4444"></span>标题
    <span class="leg-dot" style="background:#3b82f6"></span>正文
    <span class="leg-dot" style="background:#10b981"></span>公式
    <span class="leg-dot" style="background:#f59e0b"></span>表格
    <span class="leg-dot" style="background:#8b5cf6"></span>图片
  </span>
</div>
<div id="stage">
  <div id="prev-area"></div>
  <div id="curr-area"></div>
  <div id="next-area"></div>
</div>
<div id="nav-btns">
  <button onclick="prevPage()" title="上一页">⬆</button>
  <button onclick="nextPage()" title="下一页">⬇</button>
</div>
<script>{PDF_JS}</script>
<script>
var workerCode=atob('{worker_b64}');var workerBlob=new Blob([workerCode],{{type:'application/javascript'}});
pdfjsLib.GlobalWorkerOptions.workerSrc=URL.createObjectURL(workerBlob);
let pdfDoc=null,currentPage=1,totalPages=0;
let pageMapping=null,overlayVisible=true,colorMap={{
  heading:'transparent',text:'transparent',
  interline_equation:'transparent',table:'transparent',
  image:'transparent'
}};
let borderMap={{
  heading:'#ef4444',text:'#3b82f6',interline_equation:'#10b981',
  table:'#f59e0b',image:'#8b5cf6'
}};
let highlightedTexts=null;
var renderedPages={{}};
var pageWidth=0,pageHeight=0;

// 三字符组 Jaccard 相似度 (0~1)
function trigramSim(a,b){{
if(a===b)return 1;
var sa=new Set(),sb=new Set(),i;
for(i=0;i+3<=a.length;i++)sa.add(a.substring(i,i+3));
for(i=0;i+3<=b.length;i++)sb.add(b.substring(i,i+3));
if(sa.size===0||sb.size===0)return 0;
var inter=0;
sa.forEach(function(v){{if(sb.has(v))inter++;}});
return inter/(sa.size+sb.size-inter);
}}

function toggleOverlay(){{
  overlayVisible=!overlayVisible;
  var btn=document.getElementById('btn-overlay');
  if(overlayVisible){{btn.classList.add('active');}}
  else{{btn.classList.remove('active');}}
  renderPage(currentPage);
  window.parent.postMessage({{type:'overlay-toggled',visible:overlayVisible}},'*');
}}

function getPageByNum(pageNum){{
  if(!pageMapping||!pageMapping.pages)return null;
  return pageMapping.pages[pageNum-1]||null;
}}

function drawOverlay(canvas,pageNum){{
  if(!pageMapping||!overlayVisible)return;
  var page=getPageByNum(pageNum);
  if(!page||!page.blocks)return;
  var ctx=canvas.getContext('2d');
  var pw=page.page_size[0],ph=page.page_size[1];
  var sx=canvas.width/pw,sy=canvas.height/ph;
  page.blocks.forEach(function(b){{
    var bbox=b.bbox;
    var x=bbox[0]*sx,y=bbox[1]*sy,w=(bbox[2]-bbox[0])*sx,h=(bbox[3]-bbox[1])*sy;
    var fill=colorMap[b.block_type]||'rgba(128,128,128,0.15)';
    var stroke=borderMap[b.block_type]||'#888';
    ctx.fillStyle=fill;ctx.strokeStyle=stroke;ctx.lineWidth=0.5;
    ctx.fillRect(x,y,w,h);ctx.strokeRect(x,y,w,h);
    // 小标签
    if(w>40&&h>12){{
      ctx.fillStyle=stroke;ctx.font='9px sans-serif';
      ctx.fillText(b.block_type,x+2,y+11);
    }}
  }});
  drawHighlight(ctx,page,sx,sy,pageNum);
}}

function drawHighlight(ctx,page,sx,sy,pageNum){{
if(!highlightedTexts||!highlightedTexts.length||!page||!page.blocks)return;
if(pageNum!==currentPage)return;
var block,i,k,hc,sc,sb,hx,hy,hw,hh;
for(i=0;i<page.blocks.length;i++){{
var block=page.blocks[i];if(!block.spans||!block.spans.length)continue;
for(k=0;k<block.spans.length;k++){{
var s=block.spans[k];sc=(s.content||'').replace(/\\s+/g,'');
for(var hi=0;hi<highlightedTexts.length;hi++){{
hc=highlightedTexts[hi].replace(/\\s+/g,'');
if(sc&&hc&&sc.length>2&&hc.length>2&&(sc.indexOf(hc)>=0||hc.indexOf(sc)>=0)){{
sb=s.bbox||[0,0,0,0];
hx=sb[0]*sx;hy=sb[1]*sy;hw=(sb[2]-sb[0])*sx;hh=(sb[3]-sb[1])*sy;
ctx.strokeStyle='#fbbf24';ctx.lineWidth=2.5;
ctx.strokeRect(hx-1,hy-1,hw+2,hh+2);
ctx.fillStyle='rgba(251,191,36,0.25)';
ctx.fillRect(hx-1,hy-1,hw+2,hh+2);
}}
}}
}}
}}



// 使用内嵌的 PDF 数据（base64 解码为 ArrayBuffer）
var pdfB64='{pdf_b64}';
var pdfBin=atob(pdfB64);
var pdfBytes=new Uint8Array(pdfBin.length);
for(var i=0;i<pdfBin.length;i++){{pdfBytes[i]=pdfBin.charCodeAt(i);}}
pdfjsLib.getDocument({{data:pdfBytes.buffer}}).promise.then(function(pdf){{
pdfDoc=pdf;totalPages=pdf.numPages;
pdfDoc.getPage(1).then(function(p){{
var v=p.getViewport({{scale:1}});
pageWidth=window.innerWidth-40;
pageHeight=v.height*(pageWidth/v.width);
showPage(1);
}});
document.getElementById('indicator').textContent='1 / '+totalPages;
}}).catch(function(err){{
console.error('PDF.js: Failed to load PDF:',err);
document.getElementById('indicator').textContent='ERROR: '+err.message;
}});
function createPageWrap(num){{
var wrap=document.createElement('div');
wrap.className='page-wrap';wrap.id='page-'+num;
var c=document.createElement('canvas');
wrap.appendChild(c);
var ov=document.createElement('canvas');
ov.className='overlay';wrap.appendChild(ov);
var badge=document.createElement('div');
badge.className='page-num';badge.textContent='p'+num;wrap.appendChild(badge);
return wrap;
}}

function renderPage(num){{
if(!pdfDoc||num<1||num>totalPages)return;
pdfDoc.getPage(num).then(function(page){{
var v1=page.getViewport({{scale:1}});
var scale=pageWidth/v1.width;
var viewport=page.getViewport({{scale:scale}});
var wrap=renderedPages[num];
if(!wrap){{wrap=createPageWrap(num);renderedPages[num]=wrap;}}
var c=wrap.querySelector('canvas:not(.overlay)');
c.width=viewport.width;c.height=viewport.height;
c.style.width=viewport.width+'px';c.style.height=viewport.height+'px';
page.render({{canvasContext:c.getContext('2d'),viewport:viewport}});
var ov=wrap.querySelector('.overlay');
ov.width=viewport.width;ov.height=viewport.height;
ov.style.width=viewport.width+'px';ov.style.height=viewport.height+'px';
drawOverlay(ov,num);
}});
}}

function showPage(num){{
if(num<1)num=1;if(num>totalPages)num=totalPages;
currentPage=num;
document.getElementById('indicator').textContent=num+' / '+totalPages;
var prevArea=document.getElementById('prev-area');
prevArea.innerHTML='';
if(num>1){{
if(!renderedPages[num-1]){{renderPage(num-1);}}
setTimeout(function(){{if(renderedPages[num-1])prevArea.appendChild(renderedPages[num-1]);}},100);
}}
var currArea=document.getElementById('curr-area');
currArea.innerHTML='';
if(!renderedPages[num]){{renderPage(num);}}
setTimeout(function(){{if(renderedPages[num])currArea.appendChild(renderedPages[num]);}},100);
var nextArea=document.getElementById('next-area');
nextArea.innerHTML='';
if(num<totalPages){{
if(!renderedPages[num+1]){{renderPage(num+1);}}
setTimeout(function(){{if(renderedPages[num+1])nextArea.appendChild(renderedPages[num+1]);}},100);
}}
window.parent.postMessage({{type:'pdf-page',page:num}},'*');
}}

function prevPage(){{if(currentPage>1)showPage(currentPage-1);}}
function nextPage(){{if(currentPage<totalPages)showPage(currentPage+1);}}

// 键盘翻页
window.addEventListener('keydown',function(e){{
if(e.key==='ArrowDown'||e.key==='PageDown'||e.key===' '){{e.preventDefault();nextPage();}}
if(e.key==='ArrowUp'||e.key==='PageUp'){{e.preventDefault();prevPage();}}
if(e.key==='Home'){{e.preventDefault();showPage(1);}}
if(e.key==='End'){{e.preventDefault();showPage(totalPages);}}
}});

// 鼠标滚轮翻页
var wheelTimer=null;
window.addEventListener('wheel',function(e){{
e.preventDefault();
clearTimeout(wheelTimer);
wheelTimer=setTimeout(function(){{
if(e.deltaY>30)nextPage();else if(e.deltaY<-30)prevPage();
}},80);
}},{{passive:false}});

// 窗口 resize 时重新计算宽度并重新渲染
var resizeTimer=null;
window.addEventListener("resize",function(){{
clearTimeout(resizeTimer);
resizeTimer=setTimeout(function(){{
if(!pdfDoc)return;
var nw=window.innerWidth-40;
if(nw!==pageWidth){{
pageWidth=nw;
pdfDoc.getPage(1).then(function(p){{
pageHeight=p.getViewport({{scale:1}}).height*(pageWidth/p.getViewport({{scale:1}}).width);
}});
Object.keys(renderedPages).forEach(function(k){{renderedPages[k].remove();delete renderedPages[k];}});
showPage(currentPage);
}}
}},200);
}});
window.addEventListener('message',function(e){{
if(e.data&&e.data.type==='navigate')showPage(e.data.page);
if(e.data&&e.data.type==='page-mapping-data'){{
pageMapping=e.data.data;renderPage(currentPage);
}}
if(e.data&&e.data.type==='highlight-bbox'){{
highlightedTexts=e.data.texts||null;renderPage(currentPage);
}}
if(e.data&&e.data.type==='clear-highlight'){{
highlightedTexts=null;renderPage(currentPage);
}}
if(e.data&&e.data.type==='set-overlay'){{
overlayVisible=!!e.data.visible;
var btn=document.getElementById('btn-overlay');
if(overlayVisible){{btn.classList.add('active');}}else{{btn.classList.remove('active');}}
renderPage(currentPage);
}}
if(e.data&&e.data.type==='get-bbox-pos'){{
var pgEl=document.getElementById('page-'+e.data.page);
if(!pgEl||!pageMapping)return;
var pd=getPageByNum(e.data.page);
if(!pd||!pd.page_size)return;
var pr=pgEl.getBoundingClientRect();
var pw=pd.page_size[0],ph=pd.page_size[1];
// 收集页内所有 span: {{text, bbox}}
var allSpans=[];
for(var bi=0;bi<(pd.blocks||[]).length;bi++){{
var blk=pd.blocks[bi];
if(!blk.spans)continue;
for(var si=0;si<blk.spans.length;si++){{
var sp=blk.spans[si];
allSpans.push({{t:(sp.content||'').replace(/\\s+/g,''),b:sp.bbox||[0,0,0,0]}});
}}
}}
// 最大相似度匹配: 对每个 target text，找页内相似度最高的 span
var results=[];
var texts=e.data.texts||[];
var used=Array(allSpans.length).fill(false);
for(var ti=0;ti<texts.length;ti++){{
var tc=texts[ti].replace(/\\s+/g,'');
if(tc.length<3)continue;
var bestScore=0.25,bestIdx=-1;
for(var ai=0;ai<allSpans.length;ai++){{
if(used[ai])continue;
var sc=allSpans[ai].t;
if(sc.length<3)continue;
// 三字符组 Jaccard 相似度
var score=trigramSim(tc,sc);
if(score>bestScore){{bestScore=score;bestIdx=ai;}}
}}
if(bestIdx>=0){{
used[bestIdx]=true;
var b=allSpans[bestIdx].b;
results.push({{x:b[0]/pw*pr.width+pr.left,y:b[1]/ph*pr.height+pr.top,w:(b[2]-b[0])/pw*pr.width,h:(b[3]-b[1])/ph*pr.height}});
}}
}}
window.parent.postMessage({{type:'bbox-pos',page:e.data.page,pageRect:{{left:pr.left,top:pr.top,width:pr.width,height:pr.height}},ifrScrollY:window.scrollY,ifrInnerH:window.innerHeight,bboxes:results}},'*');
}}
}});
</script></body></html>"#,
            worker_b64 = base64_encode(PDF_WORKER_JS.as_bytes()),
            pdf_b64 = pdf_b64
        );
        return Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/html; charset=utf-8")
            .body(html.into_bytes())
            .unwrap();
    }

    // 原始文件请求（?raw=1 或非 PDF）
    if let Ok(mut file) = fs::File::open(&path) {
        let mut buf = Vec::new();
        if file.read_to_end(&mut buf).is_ok() {
            let mime = mime_guess::from_path(&path).first_or_octet_stream();
            return Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", mime.as_ref())
                .header("Access-Control-Allow-Origin", "*")
                .body(buf)
                .unwrap();
        }
    }
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(b"File not found".to_vec())
        .unwrap()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 初始化 PDF 渲染缓存（每个项目最多缓存 20 页）
    pdf_render::init_pdf_cache(20);

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .register_uri_scheme_protocol("narrativestructure", asset_protocol)
        .manage(ProjectState::new())
        .setup(|_app| {
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Agent Proxy v2: 前端主动轮询
            project_manager::agent_poll_queue,
            project_manager::eval_result_read,
            project_manager::import_new_project,
            project_manager::open_project,
            project_manager::close_project,
            project_manager::get_project_path,
            project_manager::get_mcp_binary_path,
            project_manager::import_document,
            project_manager::list_project_files,
            project_manager::find_asset_file,
            project_manager::read_file_bytes,
            project_manager::save_screenshot,
            project_manager::get_page_mapping_json,
            project_manager::get_page_mapping_range,
            // db_engine
            db_engine::get_toc,
            db_engine::get_blocks,
            db_engine::get_blocks_paginated,
            db_engine::get_block,
            db_engine::get_block_chunk,
            db_engine::get_blocks_by_page,
            db_engine::get_page_stats,
            db_engine::update_block,
            db_engine::search_blocks,
            db_engine::get_child_count,
            // PDF rendering with LiteParse
            pdf_render::render_pdf_pages,
            pdf_render::get_pdf_page_count,
        ])
        .run(tauri::generate_context!())
        .expect("error while running NarrativeStructure");
}
