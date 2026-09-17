// Controlled external SDK boundary. The real CRM adapter/useCall and HTTP flows
// run unchanged. This verifies signaling/state, never WebRTC, audio or SIP media.
export const RoomEvent = Object.fromEntries(['ParticipantConnected','ParticipantDisconnected',
  'ParticipantAttributesChanged','TrackSubscribed','TrackUnsubscribed','Disconnected'].map(x => [x,x]));
const endpoint = '/__e2e/call-provider';
async function send(path, data) {
  const response = await fetch(endpoint + path, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(data) });
  if (!response.ok) throw Error('Controlled LiveKit boundary rejected operation');
  return response.json();
}
export async function createLocalAudioTrack() {
  const state = await send('/microphone', {});
  if (state.denied) throw new DOMException('Controlled microphone permission denial', 'NotAllowedError');
  return { stop() {}, async mute() {}, async unmute() {} };
}
export class Room {
  handlers = new Map(); room = null; timer = null; phase = null;
  localParticipant = { publishTrack: async () => {} };
  on(event, fn) { const list = this.handlers.get(event) || []; list.push(fn); this.handlers.set(event,list); }
  emit(event, ...args) { for (const fn of this.handlers.get(event) || []) fn(...args); }
  async connect(url, token) {
    const joined = await send('/join', { url, token }); this.room = joined.room;
    const poll = async () => {
      if (!this.room) return;
      const state = await send('/state', { room: this.room });
      if (state.phase !== this.phase && ['ringing','active','ended'].includes(state.phase)) {
        const participant = { identity: state.sip, attributes: { 'sip.callStatus': state.phase === 'ended' ? 'hangup' : state.phase } };
        const previous = this.phase; this.phase = state.phase;
        if (state.phase === 'ended') this.emit(RoomEvent.ParticipantDisconnected, participant);
        else if (!['ringing','active'].includes(previous)) this.emit(RoomEvent.ParticipantConnected, participant);
        else this.emit(RoomEvent.ParticipantAttributesChanged, participant.attributes, participant);
      }
      if (this.room) this.timer = setTimeout(() => poll().catch(() => this.emit(RoomEvent.Disconnected)), 75);
    };
    await poll();
  }
  async disconnect() { clearTimeout(this.timer); this.room = null; }
}
