pub mod project_manager;
pub mod db_engine;
pub mod markdown_parser;
pub mod page_mapper;

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

    // 如果是 .pdf 请求，返回内嵌 PDF.js 连续查看器
    if path.extension().map(|e| e == "pdf").unwrap_or(false) && !uri.contains("raw=1") {
        let html = format!(r#"<!DOCTYPE html><html><head><meta charset="utf-8"><style>
body{{margin:0;background:#525659}}
#toolbar{{position:fixed;top:4px;left:8px;z-index:100;display:flex;gap:6px;align-items:center}}
#toolbar button{{background:rgba(0,0,0,0.55);color:#ccc;border:1px solid rgba(255,255,255,0.15);border-radius:3px;padding:2px 8px;font-size:11px;cursor:pointer}}
#toolbar button.active{{background:rgba(59,130,246,0.6);color:#fff;border-color:rgba(59,130,246,0.8)}}
#viewer{{padding:28px 0 8px 0;width:100%}}
.page-wrap{{position:relative;display:block;margin:0 auto 4px auto;box-shadow:0 2px 8px rgba(0,0,0,0.3)}}
.page-wrap canvas{{display:block;max-width:100%;height:auto}}
.page-wrap .overlay{{position:absolute;top:0;left:0;pointer-events:none}}
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
<div id="viewer"></div>
<script src="https://cdnjs.cloudflare.com/ajax/libs/pdf.js/3.11.174/pdf.min.js"></script>
<script>
pdfjsLib.GlobalWorkerOptions.workerSrc='https://cdnjs.cloudflare.com/ajax/libs/pdf.js/3.11.174/pdf.worker.min.js';
let pdfDoc=null,currentPage=1,autoScrolling=false,lastWidth=0,renderTimer=null;
let middleData=null,overlayVisible=true,colorMap={{
  title:'transparent',text:'transparent',
  interline_equation:'transparent',table:'transparent',
  image:'transparent'
}};
let borderMap={{
  title:'#ef4444',text:'#3b82f6',interline_equation:'#10b981',
  table:'#f59e0b',image:'#8b5cf6'
}};

function toggleOverlay(){{
  overlayVisible=!overlayVisible;
  var btn=document.getElementById('btn-overlay');
  if(overlayVisible){{btn.classList.add('active');}}
  else{{btn.classList.remove('active');}}
  if(pdfDoc)renderAllPages(pdfDoc);
}}

function drawOverlay(canvas,pageNum,viewportScale){{
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
    // 小标签
    if(w>40&&h>12){{
      ctx.fillStyle=stroke;ctx.font='9px sans-serif';
      ctx.fillText(b.type,x+2,y+11);
    }}
  }});
}}

pdfjsLib.getDocument('{pdf_url}').promise.then(function(pdf){{
pdfDoc=pdf;
document.getElementById('indicator').textContent='1 / '+pdf.numPages;
renderAllPages(pdf);
}});
function scrollToPage(num){{
autoScrolling=true;
var el=document.getElementById('page-'+num);
if(el){{el.scrollIntoView({{behavior:'smooth',block:'start'}});}}
else{{window.scrollTo(0,0);}}
setTimeout(function(){{autoScrolling=false;}},1000);
}}
function renderAllPages(pdf){{
var viewer=document.getElementById('viewer');
var containerWidth=viewer.clientWidth-16;
lastWidth=containerWidth;
viewer.innerHTML='';
for(var i=1;i<=pdf.numPages;i++){{
(function(num){{
pdf.getPage(num).then(function(page){{
var scale=containerWidth/page.getViewport({{scale:1}}).width;
var viewport=page.getViewport({{scale:scale}});
// 页面容器
var wrap=document.createElement('div');
wrap.className='page-wrap';
wrap.id='page-'+num;
// PDF canvas
var c=document.createElement('canvas');
c.width=viewport.width;c.height=viewport.height;
var ctx=c.getContext('2d');
wrap.appendChild(c);
page.render({{canvasContext:ctx,viewport:viewport}});
// 信息层 overlay canvas
var ov=document.createElement('canvas');
ov.className='overlay';
ov.width=viewport.width;ov.height=viewport.height;
wrap.appendChild(ov);
drawOverlay(ov,num,scale);
viewer.appendChild(wrap);
}});
}})(i);
}}
}}
if(window.ResizeObserver){{
new ResizeObserver(function(entries){{
var w=entries[0].contentRect.width-16;
if(Math.abs(w-lastWidth)>10&&pdfDoc){{
clearTimeout(renderTimer);
renderTimer=setTimeout(function(){{renderAllPages(pdfDoc);}},300);
}}
}}).observe(document.getElementById('viewer'));
}}
var observer=new IntersectionObserver(function(entries){{
var best=null;
entries.forEach(function(e){{
if(e.isIntersecting&&!autoScrolling){{
var id=e.target.id;
if(id&&id.startsWith('page-')){{
var p=parseInt(id.replace('page-',''));
if(!best||e.intersectionRatio>best.ratio){{best={{page:p,ratio:e.intersectionRatio}};}}
}}
}}
}});
if(best&&best.page!==currentPage){{
currentPage=best.page;
document.getElementById('indicator').textContent=best.page+' / '+(pdfDoc?pdfDoc.numPages:'?');
window.parent.postMessage({{type:'pdf-page',page:best.page}},'*');
}}
}},{{threshold:[0,0.25,0.5,0.75]}});
window.addEventListener('message',function(e){{
if(e.data&&e.data.type==='navigate')scrollToPage(e.data.page);
if(e.data&&e.data.type==='middle-data'){{
middleData=e.data.data;
if(pdfDoc)renderAllPages(pdfDoc);
}}
}});
var checkTimer=setInterval(function(){{
document.querySelectorAll('.page-wrap').forEach(function(w){{observer.observe(w);}});
}},500);
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
    match fs::File::open(&path) {
        Ok(mut file) => {
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
        _ => {}
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
        .invoke_handler(tauri::generate_handler![
            // project_manager
            project_manager::import_new_project,
            project_manager::open_project,
            project_manager::close_project,
            project_manager::get_project_path,
            project_manager::import_document,
            project_manager::list_project_files,
            project_manager::find_asset_file,
            project_manager::read_file_bytes,
            // db_engine
            db_engine::get_toc,
            db_engine::get_blocks,
            db_engine::get_blocks_paginated,
            db_engine::get_block,
            db_engine::get_block_chunk,
            db_engine::get_blocks_by_page,
            db_engine::update_block,
            db_engine::search_blocks,
            db_engine::get_child_count,
        ])
        .run(tauri::generate_context!())
        .expect("error while running NarrativeStructure");
}
