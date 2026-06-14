pub mod project_manager;
pub mod db_engine;
pub mod markdown_parser;
pub mod page_mapper;
pub mod mcp;

use project_manager::ProjectState;
use tauri::http::{Request, Response, StatusCode};

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

    // 如果是 .pdf 请求，返回按需渲染的 PDF.js 查看器 (支持单页/双页布局)
    if path.extension().map(|e| e == "pdf").unwrap_or(false) && !uri.contains("raw=1") {
        let html = format!(r#"<!DOCTYPE html><html><head><meta charset="utf-8"><style>
body{{margin:0;background:#525659;overflow:hidden}}
#toolbar{{position:fixed;top:4px;left:8px;z-index:100;display:flex;gap:6px;align-items:center}}
#toolbar button{{background:rgba(0,0,0,0.55);color:#ccc;border:1px solid rgba(255,255,255,0.15);border-radius:3px;padding:2px 8px;font-size:11px;cursor:pointer}}
#toolbar button.active{{background:rgba(59,130,246,0.6);color:#fff;border-color:rgba(59,130,246,0.8)}}
#viewer{{position:absolute;top:0;left:0;width:100%;height:100%;display:flex;justify-content:center;align-items:center}}
/* 单页模式 */
.layout-single .page-wrap{{position:relative;display:block;margin:0 auto;box-shadow:0 2px 8px rgba(0,0,0,0.3)}}
/* 双页横排模式 */
.layout-double-h #page-container{{display:flex;flex-direction:row;gap:16px;align-items:flex-start}}
.layout-double-h .page-wrap{{position:relative;display:inline-block;box-shadow:0 2px 8px rgba(0,0,0,0.3)}}
/* 双页竖排模式 */
.layout-double-v #page-container{{display:flex;flex-direction:column;gap:16px;align-items:center}}
.layout-double-v .page-wrap{{position:relative;display:block;box-shadow:0 2px 8px rgba(0,0,0,0.3)}}
.page-wrap canvas{{display:block;width:100%;height:auto}}
.page-wrap .overlay{{position:absolute;top:0;left:0;pointer-events:none}}
.page-num{{position:absolute;top:5px;right:8px;background:rgba(0,0,0,0.55);color:#ccc;padding:1px 6px;border-radius:3px;font-size:10px;font-family:monospace;pointer-events:none;z-index:5;user-select:none}}
#indicator{{position:fixed;top:4px;right:8px;background:rgba(0,0,0,0.6);color:#ccc;padding:2px 8px;border-radius:3px;font-size:11px;z-index:10}}
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
<div id="viewer"><div id="page-container"></div></div>
<script src="https://cdnjs.cloudflare.com/ajax/libs/pdf.js/3.11.174/pdf.min.js"></script>
<script>
pdfjsLib.GlobalWorkerOptions.workerSrc='https://cdnjs.cloudflare.com/ajax/libs/pdf.js/3.11.174/pdf.worker.min.js';
let pdfDoc=null, currentPage=0;
let middleData=null, overlayVisible=true;
let colorMap={{title:'transparent',text:'transparent',interline_equation:'transparent',table:'transparent',image:'transparent'}};
let borderMap={{title:'#ef4444',text:'#3b82f6',interline_equation:'#10b981',table:'#f59e0b',image:'#8b5cf6'}};
let highlightedTexts=null;

// 从 URL 解析布局和目标页码
var params=new URLSearchParams(window.location.search);
var layout=params.get('layout')||'single';   // single | double-h | double-v
var targetPage=parseInt(params.get('page'))||1;
var pagesRange=params.get('pages');            // "3-4" 格式
var pageStart=1, pageEnd=1;
if(pagesRange){{var parts=pagesRange.split('-');pageStart=parseInt(parts[0]);pageEnd=parseInt(parts[1]);}}
else if(layout!=='single'){{pageStart=targetPage;pageEnd=Math.min(targetPage+1,pdfDoc?pdfDoc.numPages:targetPage+1);}}
else{{pageStart=targetPage;pageEnd=targetPage;}}

function trigramSim(a,b){{
if(a===b)return 1;
var sa=new Set(),sb=new Set(),i;
for(i=0;i+3<=a.length;i++)sa.add(a.substring(i,i+3));
for(i=0;i+3<=b.length;i++)sb.add(b.substring(i,i+3));
if(sa.size===0||sb.size===0)return 0;
var inter=0;sa.forEach(function(v){{if(sb.has(v))inter++;}});
return inter/(sa.size+sb.size-inter);
}}

function toggleOverlay(){{
overlayVisible=!overlayVisible;
var btn=document.getElementById('btn-overlay');
btn.classList.toggle('active',overlayVisible);
renderCurrentPages();
window.parent.postMessage({{type:'overlay-toggled',visible:overlayVisible}},'*');
}}

function drawOverlay(canvas,pageNum){{
if(!middleData||!overlayVisible)return;
var page=middleData[pageNum-1];
if(!page||!page.para_blocks)return;
var ctx=canvas.getContext('2d');
var pw=page.page_size[0],ph=page.page_size[1];
var sx=canvas.width/pw,sy=canvas.height/ph;
page.para_blocks.forEach(function(b){{
var bbox=b.bbox;
var x=bbox[0]*sx,y=bbox[1]*sy,w=(bbox[2]-bbox[0])*sx,h=(bbox[3]-bbox[1])*sy;
var fill=colorMap[b.type]||'rgba(128,128,128,0.15)';
var stroke=borderMap[b.type]||'#888';
ctx.fillStyle=fill;ctx.strokeStyle=stroke;ctx.lineWidth=0.5;
ctx.fillRect(x,y,w,h);ctx.strokeRect(x,y,w,h);
if(w>40&&h>12){{ctx.fillStyle=stroke;ctx.font='9px sans-serif';ctx.fillText(b.type,x+2,y+11);}}
}});
drawHighlight(ctx,page,sx,sy,pageNum);
}}

function drawHighlight(ctx,page,sx,sy,pageNum){{
if(!highlightedTexts||!highlightedTexts.length||!page||!page.para_blocks)return;
if(pageNum!==currentPage)return;
var pb,i,j,k,hc,sc,sb,hx,hy,hw,hh;
for(i=0;i<page.para_blocks.length;i++){{
pb=page.para_blocks[i];if(!pb.lines)continue;
for(j=0;j<pb.lines.length;j++){{
var l=pb.lines[j];if(!l.spans)continue;
for(k=0;k<l.spans.length;k++){{
var s=l.spans[k];sc=(s.content||'').replace(/\\s+/g,'');
for(var hi=0;hi<highlightedTexts.length;hi++){{
hc=highlightedTexts[hi].replace(/\\s+/g,'');
if(sc&&hc&&sc.length>2&&hc.length>2&&(sc.indexOf(hc)>=0||hc.indexOf(sc)>=0)){{
sb=s.bbox||[0,0,0,0];
hx=sb[0]*sx;hy=sb[1]*sy;hw=(sb[2]-sb[0])*sx;hh=(sb[3]-sb[1])*sy;
ctx.strokeStyle='#fbbf24';ctx.lineWidth=2.5;
ctx.strokeRect(hx-1,hy-1,hw+2,hh+2);
ctx.fillStyle='rgba(251,191,36,0.25)';ctx.fillRect(hx-1,hy-1,hw+2,hh+2);
}}}}}}}}
}}

function renderCurrentPages(){{
if(!pdfDoc)return;
var container=document.getElementById('page-container');
var viewer=document.getElementById('viewer');
container.innerHTML='';
// 设置布局 class
viewer.className='layout-'+layout;
var containerW=viewer.clientWidth;
var containerH=viewer.clientHeight;

var pagesToRender=[];
for(var p=pageStart;p<=pageEnd;p++){{if(p>=1&&p<=pdfDoc.numPages)pagesToRender.push(p);}}
if(!pagesToRender.length)return;

var scale;
if(layout==='single'){{
scale=Math.min(containerW*0.95,containerH*0.95)/400; // 单页适配
}}else{{
if(layout==='double-h'){{
scale=Math.min(containerW*0.48/400,containerH*0.95/600);
}}else{{
scale=Math.min(containerW*0.9/400,containerH*0.48/600);
}}
}}

var pending=pagesToRender.length;
pagesToRender.forEach(function(num){{
pdf.getPage(num).then(function(page){{
var vp=page.getViewport({{scale:scale}});
var wrap=document.createElement('div');
wrap.className='page-wrap';
wrap.id='page-'+num;
var c=document.createElement('canvas');
c.width=vp.width;c.height=vp.height;
wrap.appendChild(c);
page.render({{canvasContext:c.getContext('2d'),viewport:vp}});
var ov=document.createElement('canvas');
ov.className='overlay';
ov.width=vp.width;ov.height=vp.height;
wrap.appendChild(ov);
drawOverlay(ov,num,scale);
var badge=document.createElement('div');
badge.className='page-num';
badge.textContent='p'+num;
wrap.appendChild(badge);
container.appendChild(wrap);
if(--pending===0){{
currentPage=pageStart;
document.getElementById('indicator').textContent=pageStart+' / '+(pdfDoc?pdfDoc.numPages:'?');
window.parent.postMessage({{type:'pdf-page',page:currentPage}},'*');
}}
}});
}});
}}

pdfjsLib.getDocument('{pdf_url}').promise.then(function(pdf){{
pdfDoc=pdf;
// 更新 pageEnd (双页模式需要实际页数)
if(layout!=='single'){{pageEnd=Math.min(pageStart+1,pdf.numPages);}}
renderCurrentPages();
}});

window.addEventListener('message',function(e){{
if(!e.data)return;
// 导航到指定页
if(e.data.type==='navigate'){{
var newPage=e.data.page;
if(layout==='single'){{pageStart=newPage;pageEnd=newPage;}}
else{{pageStart=newPage;pageEnd=Math.min(newPage+1,pdfDoc?pdfDoc.numPages:newPage+1);}}
if(pdfDoc)renderCurrentPages();
}}
// 接收 middle-data
if(e.data.type==='middle-data'){{
middleData=e.data.data;
if(pdfDoc)renderCurrentPages();
}}
// 高亮
if(e.data.type==='highlight-bbox'){{
highlightedTexts=e.data.texts||null;
if(pdfDoc)renderCurrentPages();
}}
if(e.data.type==='clear-highlight'){{
highlightedTexts=null;
if(pdfDoc)renderCurrentPages();
}}
// 显示/隐藏信息层
if(e.data.type==='set-overlay'){{
overlayVisible=!!e.data.visible;
var btn=document.getElementById('btn-overlay');
btn.classList.toggle('active',overlayVisible);
if(pdfDoc)renderCurrentPages();
}}
// 切换布局
if(e.data.type==='set-layout'){{
layout=e.data.layout||'single';
if(e.data.page){{
if(layout==='single'){{pageStart=e.data.page;pageEnd=e.data.page;}}
else{{pageStart=e.data.page;pageEnd=Math.min(e.data.page+1,pdfDoc?pdfDoc.numPages:e.data.page+1);}}
}}
if(pdfDoc)renderCurrentPages();
}}
// 获取 bbox 位置
if(e.data.type==='get-bbox-pos'){{
var pgEl=document.getElementById('page-'+e.data.page);
if(!pgEl||!middleData)return;
var pd=middleData[e.data.page-1];
if(!pd||!pd.page_size)return;
var pr=pgEl.getBoundingClientRect();
var pw=pd.page_size[0],ph=pd.page_size[1];
var allSpans=[],bi,li,si;
for(bi=0;bi<(pd.para_blocks||[]).length;bi++){{
var pb=pd.para_blocks[bi];
for(li=0;li<(pb.lines||[]).length;li++){{
var l=pb.lines[li];
for(si=0;si<(l.spans||[]).length;si++){{
var sp=l.spans[si];
allSpans.push({{t:(sp.content||'').replace(/\\s+/g,''),b:sp.bbox||[0,0,0,0]}});
}}}}}}
var results=[],texts=e.data.texts||[];
var used=Array(allSpans.length).fill(false);
for(var ti=0;ti<texts.length;ti++){{
var tc=texts[ti].replace(/\\s+/g,'');
if(tc.length<3)continue;
var bestScore=0.25,bestIdx=-1;
for(var ai=0;ai<allSpans.length;ai++){{
if(used[ai])continue;
var sc=allSpans[ai].t;
if(sc.length<3)continue;
var score=trigramSim(tc,sc);
if(score>bestScore){{bestScore=score;bestIdx=ai;}}
}}
if(bestIdx>=0){{
used[bestIdx]=true;
var b=allSpans[bestIdx].b;
results.push({{x:b[0]/pw*pr.width+pr.left,y:b[1]/ph*pr.height+pr.top,w:(b[2]-b[0])/pw*pr.width,h:(b[3]-b[1])/ph*pr.height}});
}}}}
window.parent.postMessage({{type:'bbox-pos',page:e.data.page,pageRect:{{left:pr.left,top:pr.top,width:pr.width,height:pr.height}},ifrScrollY:0,ifrInnerH:window.innerHeight,bboxes:results}},'*');
}}
}});
</script></body></html>"#,
            pdf_url = format!("narrativestructure://localhost/{}?raw=1", path_str)
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running NarrativeStructure");
}