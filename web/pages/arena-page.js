import { mountShadow } from "../lib/component.js";
import { requestSnapshot } from "../lib/api.js";
import "../components/match-controls.js";
import "../components/match-scoreboard.js";
import "../components/arena-board.js";
import "../components/robot-roster.js";
import "../components/match-result.js";

class ArenaPage extends HTMLElement {
  constructor() {
    super();
    this.root = mountShadow(this, "/pages/arena-page.css", `
      <div class="page-shell">
        <header class="masthead"><a href="/" class="brand" aria-label="BOT ARENA 홈"><img src="/assets/arena.svg" width="34" height="34" alt=""><span>BOT<span class="brand-light"> ARENA</span></span></a><div class="header-right"><span class="local-label">LOCAL PLAYGROUND</span><span class="connection"><i aria-hidden="true"></i><span class="connection-label">서버 연결 중</span></span></div></header>
        <main>
          <div class="hero"><div><div class="hero-eyebrow"><span class="tiny-line"></span>CODE. COLLECT. COMPETE.</div><h1>캠퍼스 수거 로봇 대전<span class="heading-dot">.</span></h1><p>작은 로봇, 다른 전략. 재활용품 96개를 두고 펼치는 두 봇의 대결.</p></div><div class="match-label"><span class="match-state" data-status="loading"><i aria-hidden="true"></i><span>연결 중</span></span><span class="match-id">LOCAL MATCH</span></div></div>
          <div class="connection-error" role="alert" hidden><span aria-hidden="true">!</span><p></p></div>
          <match-scoreboard></match-scoreboard>
          <match-result hidden></match-result>
          <div class="match-layout"><arena-board></arena-board><aside aria-label="관전 제어와 로봇 상태"><match-controls></match-controls><robot-roster></robot-roster><div class="rule-note"><span aria-hidden="true">↳</span><p>수거한 자원은 자기 팀 반납 구역에서 자동으로 점수가 됩니다.<strong>최대 200턴, 더 많이 반납한 팀이 승리합니다.</strong></p></div></aside></div>
          <div class="how-to"><span class="eyebrow">HOW IT WORKS</span><span><i>01</i>자원으로 이동</span><b aria-hidden="true">→</b><span><i>02</i>최대 4개 수거</span><b aria-hidden="true">→</b><span><i>03</i>자기 구역에 반납</span><div>재활용품 1개 = 1점</div></div>
        </main>
        <footer><span>BOT ARENA <span class="footer-divider">/</span> 전략을 코드로, 결과를 경기로.</span><span>Python bots <i>×</i> Rust engine</span></footer>
      </div>`);
    this.controls = this.root.querySelector("match-controls");
    this.scoreboard = this.root.querySelector("match-scoreboard");
    this.board = this.root.querySelector("arena-board");
    this.roster = this.root.querySelector("robot-roster");
    this.result = this.root.querySelector("match-result");
    this._snapshot = null;
    this._epoch = 0;
    this._requestSerial = 0;
    this._acceptedSerial = 0;
    this._pollTimer = null;
    this._controller = null;
    this._onStart = event => this.mutate("start", event.detail.turnDelay);
    this._onRestart = event => this.mutate("restart", event.detail.turnDelay);
    this._onSpeed = event => this.mutate("speed", event.detail.turnDelay);
    this._onPause = () => this.mutate("pause");
    this._onResume = () => this.mutate("resume");
    this._onStep = () => this.mutate("step");
  }

  connectedCallback() {
    this._pending = false;
    this.controls.pending = false;
    this.addEventListener("match-start", this._onStart);
    this.addEventListener("match-restart", this._onRestart);
    this.addEventListener("match-speed", this._onSpeed);
    this.addEventListener("match-pause", this._onPause);
    this.addEventListener("match-resume", this._onResume);
    this.addEventListener("match-step", this._onStep);
    const epoch = this.invalidate();
    this.poll(epoch);
  }

  disconnectedCallback() {
    this.removeEventListener("match-start", this._onStart);
    this.removeEventListener("match-restart", this._onRestart);
    this.removeEventListener("match-speed", this._onSpeed);
    this.removeEventListener("match-pause", this._onPause);
    this.removeEventListener("match-resume", this._onResume);
    this.removeEventListener("match-step", this._onStep);
    this.invalidate();
  }

  invalidate() {
    this._epoch += 1;
    clearTimeout(this._pollTimer);
    this._pollTimer = null;
    this._controller?.abort();
    this._controller = null;
    return this._epoch;
  }

  current(epoch) { return this.isConnected && epoch === this._epoch; }

  async poll(epoch) {
    if (!this.current(epoch)) return;
    this._controller = new AbortController();
    const serial = ++this._requestSerial;
    let failed = false;
    try {
      const snapshot = await requestSnapshot("/api/match", { signal: this._controller.signal });
      if (this.current(epoch)) { this.applySnapshot(snapshot, serial); this.showConnectionError(""); }
    } catch (error) {
      if (this.current(epoch) && error.name !== "AbortError") { failed = true; this.showConnectionError(error.message); }
    } finally {
      if (this.current(epoch)) {
        this._controller = null;
        // One recursive timer: requests never overlap, and disconnect/restart cancels it.
        this._pollTimer = setTimeout(() => this.poll(epoch), failed ? 1200 : this._snapshot?.status === "running" ? 100 : 700);
      }
    }
  }

  async mutate(action, turnDelay) {
    if (this._pending) return;
    const epoch = this.invalidate();
    const serial = ++this._requestSerial;
    this._pending = true;
    this.controls.pending = true;
    this._controller = new AbortController();
    try {
      const body = ["start", "restart", "speed"].includes(action) ? { turn_delay_ms: turnDelay } : undefined;
      if (action === "restart") body.paused = Boolean(this._snapshot?.paused);
      const snapshot = await requestSnapshot(`/api/match/${action}`, {
        method: "POST", body, signal: this._controller.signal,
      });
      if (this.current(epoch)) { this.applySnapshot(snapshot, serial); this.showConnectionError(""); }
    } catch (error) {
      if (this.current(epoch) && error.name !== "AbortError") this.showConnectionError(error.message);
    } finally {
      if (this.current(epoch)) {
        this._pending = false;
        this.controls.pending = false;
        this._controller = null;
        this._pollTimer = setTimeout(() => this.poll(epoch), 100);
      }
    }
  }

  applySnapshot(snapshot, serial) {
    if (serial < this._acceptedSerial) return;
    // A turn may advance during a request. Never replace the same match with an older turn.
    if (this._snapshot?.match_id != null && snapshot.match_id === this._snapshot.match_id && snapshot.state.completed_turn < this._snapshot.state.completed_turn) return;
    this._acceptedSerial = serial;
    this._snapshot = snapshot;
    this.controls.snapshot = snapshot;
    this.scoreboard.snapshot = snapshot;
    this.board.state = snapshot.state;
    this.roster.state = snapshot.state;
    this.result.snapshot = snapshot;
    const status = this.root.querySelector(".match-state");
    const paused = snapshot.status === "running" && snapshot.paused;
    status.dataset.status = paused ? "paused" : snapshot.status;
    status.querySelector("span").textContent = paused ? "경기 일시정지" : { idle: "시작 대기", running: "경기 진행 중", finished: "경기 종료", failed: "실행 오류" }[snapshot.status];
    this.root.querySelector(".match-id").textContent = snapshot.match_id == null ? "LOCAL MATCH" : `MATCH ${String(snapshot.match_id).padStart(3, "0")}`;
  }

  showConnectionError(message) {
    const error = this.root.querySelector(".connection-error");
    error.hidden = !message;
    error.querySelector("p").textContent = message;
    const connection = this.root.querySelector(".connection");
    connection.classList.toggle("offline", Boolean(message));
    connection.querySelector("span").textContent = message ? "연결 확인 필요" : "로컬 서버 연결됨";
  }
}

customElements.define("arena-page", ArenaPage);
