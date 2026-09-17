// Deterministic inference boundary. CRM state/authorization/tool results remain real.
let plan = null, round = 0;
const send = (res, status, body) => { res.writeHead(status, { 'content-type': 'application/json' }); res.end(JSON.stringify(body)); };
export async function handleProvider(req, res, body, requests) {
  if (!['/operator-control', '/openai/v1/chat/completions'].includes(req.url)) return false;
  if (req.method !== 'POST') { send(res, 405, { error: 'method_not_allowed' }); return true; }
  if (typeof body === 'string' || Buffer.isBuffer(body)) body = JSON.parse(String(body));
  if (req.url === '/operator-control') {
    if (body.scenario === 'call' && body.personId) body = { ...body, calls: [{ name: 'start_call', arguments: {
      person_id: body.personId, ...(body.contactMethodId ? { contact_method_id: body.contactMethodId } : {}),
    } }], reply: 'Review the server call proposal before confirming.' };
    if (!Array.isArray(body.calls) || body.calls.length > 3) { send(res, 400, { error: 'invalid_plan' }); return true; }
    plan = body; round = 0; send(res, 200, { ready: true }); return true;
  }
  if (!plan) { send(res, 503, { error: 'no_script_installed' }); return true; }
  requests.push({ kind: 'inference', at: Date.now(), scenario: plan.scenario, round,
    toolNames: round === 0 ? plan.calls.map(c => c.name) : [],
    // Deliberately retain no prompt, message, tool arguments, tool bodies or credentials.
    toolResultCount: body.messages.filter(m => m.role === 'tool').length,
    status: plan.failure ? 429 : 200 });
  if (plan.failure) { send(res, 429, { error: { message: 'Synthetic rate limit' } }); return true; }
  const message = round++ === 0 && plan.calls.length ? { content: null, tool_calls: plan.calls.map((call, i) => ({
    id: 'e2e-tool-' + i, type: 'function', function: { name: call.name, arguments: JSON.stringify(call.arguments) },
  })) } : { content: plan.reply ?? 'The requested server tools have finished.' };
  send(res, 200, { choices: [{ message }], usage: { prompt_tokens: 10, completion_tokens: 10 } }); return true;
}
