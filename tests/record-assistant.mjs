import { chromium } from 'playwright-core';
import { spawn, execFileSync } from 'node:child_process';
import { mkdtemp, readdir, readFile, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import {setTimeout as wait} from 'node:timers/promises';
import assert from 'node:assert/strict';

const dir=await mkdtemp(path.join(tmpdir(),'jarvis-film-'));
const app=spawn(path.resolve('src-tauri/target/release/JARVIS.exe'),[],{env:{...process.env,WEBVIEW2_USER_DATA_FOLDER:dir,WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:'--remote-debugging-address=127.0.0.1 --remote-debugging-port=0'}});
let browser;
try{
 let port;
 for(let i=0;i<100&&!port;i++){
   const file=(await readdir(dir,{recursive:true})).find(f=>path.basename(f)==='DevToolsActivePort');
   if(file)port=parseInt(await readFile(path.join(dir,file),'utf8'));
   if(!port)await wait(200);
 }
 browser=await chromium.connectOverCDP('http://127.0.0.1:'+port);
 const page=browser.contexts()[0].pages()[0];
 await page.waitForURL(url=>url.hostname==='tauri.localhost');
 await page.locator('canvas').waitFor();
 await wait(850);
 // Mirror only the application's own audio output into a temporary recorder.
 await page.evaluate(()=>{
   const original=AudioNode.prototype.connect;
   AudioNode.prototype.connect=function(target,...rest){
     if(target===this.context.destination&&!window.recordAudio){
       const tap=this.context.createMediaStreamDestination();
       original.call(this,tap);
       const recorder=new MediaRecorder(tap.stream),chunks=[];
       recorder.ondataavailable=e=>chunks.push(e.data);
       window.finishAudio=new Promise(resolve=>{recorder.onstop=async()=>resolve(Array.from(new Uint8Array(await new Blob(chunks).arrayBuffer())))});
       window.recordAudio=recorder;
       recorder.start();
       window.audioStart=performance.now();
     }
     return original.call(this,target,...rest);
   }
 });
 const video=path.join(dir,'assistant.webm');
 await page.screencast.start({path:video,size:{width:1280,height:720},fps:25});
 const start=await page.evaluate(()=>performance.now());
 await wait(1200);
 const button=name=>page.getByRole('button',{name,exact:true});
 const state=name=>page.locator('main[data-state="'+name+'"]').waitFor();
 await button('Simular escuta').click();await state('listening');await wait(1900);
 const field=page.getByRole('textbox',{name:'Mensagem'});
 await field.pressSequentially('Como funciona o núcleo?',{delay:55});
 await button('Enviar').click();await state('responding');await wait(2300);
 await field.pressSequentially('alerta',{delay:130});
 await button('Enviar').click();await state('alert');await wait(1300);
 await button('Modo imersivo').click();await state('idle');await wait(1600);
 await button('Sair do modo imersivo').click();await wait(1100);
 await page.screencast.stop();
 const {bytes,offset}=await page.evaluate(async start=>{window.recordAudio.stop();return {bytes:await window.finishAudio,offset:(window.audioStart-start)/1000}},start);
 assert.ok(bytes.length>0);
 const audio=path.join(dir,'audio.webm');await writeFile(audio,Buffer.from(bytes));
 const output=path.resolve(process.argv[3]??'docs/JARVIS-teste.mp4');
 execFileSync(process.argv[2]??'ffmpeg',['-y','-loglevel','error','-i',video,'-itsoffset',String(offset),'-i',audio,'-map','0:v','-map','1:a','-c:v','libx264','-pix_fmt','yuv420p','-crf','20','-c:a','aac','-movflags','+faststart',output]);
 console.log(output);
}finally{await browser?.close();app.kill()}
