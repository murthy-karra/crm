import { createHmac, timingSafeEqual } from 'node:crypto';
const rooms = new Map();
let mode = 'answered';
const send = (res,status,body) => { res.writeHead(status, {'content-type':'application/json'}); res.end(JSON.stringify(body)); };
function claims(token) {
  const [head, body, signature] = token.split('.');
  const actual = Buffer.from(signature || '', 'base64url');
  const expected = createHmac('sha256',process.env.E2E_LIVEKIT_SECRET).update(head+'.'+body).digest();
  if (actual.length!==expected.length || !timingSafeEqual(actual,expected)) throw Error('Invalid provider signature');
  const value = JSON.parse(Buffer.from(body,'base64url'));
  if (value.exp < Date.now()/1000) throw Error('Expired provider token');
  return value;
}
export async function handleProvider(req,res,body,requests) {
  const path=req.url.split('?')[0];
  if (path==='/call-control') {
    if (body.mode) mode=body.mode;
    if (body.action==='answer') {
      const room=rooms.get(body.room);
      if (!room?.pending) return send(res,409,{error:'not_ringing'}),true;
      room.phase='active'; room.participants.add(room.sip);
      send(room.pending,200,{sipCallId:'synthetic-'+body.room}); room.pending=null;
    }
    send(res,200,{mode}); return true;
  }
  if (path.startsWith('/call-provider/')) {
    if (path.endsWith('/microphone')) { send(res,200,{denied:mode==='microphone_denied'}); return true; }
    if (path.endsWith('/join')) {
      const grant=claims(body.token); const room=rooms.get(grant.video.room);
      if (!room || !grant.video.roomJoin || !grant.sub?.startsWith('agent:')) return send(res,403,{}),true;
      room.participants.add(grant.sub);
      requests.push({kind:'livekit_browser',action:'join',room:grant.video.room,at:Date.now()});
      send(res,200,{room:grant.video.room}); return true;
    }
    const room=rooms.get(body.room); send(res,room?200:404,room?{phase:room.phase,sip:room.sip}:{}); return true;
  }
  if (!path.startsWith('/twirp/')) return false;
  const auth=claims((req.headers.authorization||'').replace(/^Bearer /,''));
  if(auth.iss!==process.env.E2E_LIVEKIT_KEY) return send(res,401,{}),true;
  const method=path.split('/').at(-1);
  requests.push({kind:'livekit',method,room:body.room||body.roomName||body.name,at:Date.now()});
  if(method==='CreateRoom') {
    if(mode==='provider_error') return send(res,503,{code:'unavailable'}),true;
    rooms.set(body.name,{participants:new Set(),phase:'joining',sip:'sip:'+body.name.replace('call:',''),pending:null}); send(res,200,{name:body.name});
  } else if(method==='ListParticipants') send(res,200,{participants:[...(rooms.get(body.room)?.participants||[])].map(identity=>({identity}))});
  else if(method==='CreateSIPParticipant') {
    const room=rooms.get(body.roomName); room.phase='ringing'; room.sip=body.participantIdentity;
    if(mode==='busy') { room.phase='ended'; send(res,400,{code:'failed_precondition',meta:{sip_status_code:'486'}}); }
    else { room.pending=res; req.on('close',()=>{ if(res.destroyed && room.pending===res) room.pending=null; }); }
  } else if(method==='DeleteRoom') {
    const room=rooms.get(body.room); if(room) {room.phase='ended';room.participants.clear();if(room.pending){send(room.pending,400,{code:'cancelled',meta:{sip_status_code:'487'}});room.pending=null;}} send(res,200,{});
  } else send(res,404,{code:'unimplemented'});
  return true;
}
