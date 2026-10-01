import { emit, mountShadow } from "../lib/component.js";

class MatchControls extends HTMLElement {
  constructor() {
    super();
    this.root = mountShadow(this, "/components/match-controls.css", `
      <section aria-labelledby="controls-title">
        <div class="section-heading"><span class="eyebrow">MATCH CONTROL</span><span class="control-icon" aria-hidden="true">↗</span></div>
        <h2 id="controls-title">다음 전략을 지켜보세요.</h2>
        <p class="description">두 예제 봇이 같은 경기장에서<br>각자의 수거 전략을 펼칩니다.</p>
        <button class="start" type="button"><span class="play" aria-hidden="true">▶</span><span class="start-label">경기 시작</span></button>
        <div class="playback-row">
          <button class="pause" type="button"><span class="pause-icon" aria-hidden="true">Ⅱ</span><span class="pause-label">일시정지</span></button>
          <button class="step" type="button"><span aria-hidden="true">▸│</span> 한 턴 진행</button>
        </div>
        <p class="playback-note" aria-live="polite">한 턴 진행으로 경기를 시작할 수 있습니다.</p>
        <button class="restart" type="button"><span aria-hidden="true">↻</span> 새 경기로 재시작</button>
        <div class="speed-row"><label for="speed">관전 속도</label><select id="speed" aria-label="관전 속도"><option value="1000">1초/턴</option><option value="450">천천히 · 0.45초/턴</option><option value="150" selected>보통 · 0.15초/턴</option><option value="40">빠르게 · 0.04초/턴</option></select></div>
        <p class="speed-note">속도는 경기의 판정 결과에 영향을 주지 않습니다.</p>
      </section>`);
    this.start = this.root.querySelector(".start");
    this.pause = this.root.querySelector(".pause");
    this.step = this.root.querySelector(".step");
    this.restart = this.root.querySelector(".restart");
    this.speed = this.root.querySelector("select");
    this._snapshot = null;
    this._pending = false;
    this._onStart = () => emit(this, "match-start", { turnDelay: Number(this.speed.value) });
    this._onRestart = () => emit(this, "match-restart", { turnDelay: Number(this.speed.value) });
    this._onPause = () => emit(this, this._snapshot?.paused ? "match-resume" : "match-pause");
    this._onStep = () => emit(this, "match-step");
    this._onSpeed = () => emit(this, "match-speed", { turnDelay: Number(this.speed.value) });
  }

  connectedCallback() {
    this.start.addEventListener("click", this._onStart);
    this.restart.addEventListener("click", this._onRestart);
    this.pause.addEventListener("click", this._onPause);
    this.step.addEventListener("click", this._onStep);
    this.speed.addEventListener("change", this._onSpeed);
    this.render();
  }

  disconnectedCallback() {
    this.start.removeEventListener("click", this._onStart);
    this.restart.removeEventListener("click", this._onRestart);
    this.pause.removeEventListener("click", this._onPause);
    this.step.removeEventListener("click", this._onStep);
    this.speed.removeEventListener("change", this._onSpeed);
  }

  set snapshot(value) { this._snapshot = value; this.render(); }
  set pending(value) { this._pending = value; this.render(); }

  render() {
    const status = this._snapshot?.status;
    const paused = status === "running" && this._snapshot.paused;
    this.start.disabled = this._pending || !status || status === "running";
    this.restart.disabled = this._pending || !status || status === "idle";
    this.pause.disabled = this._pending || status !== "running";
    this.step.disabled = this._pending || !(status === "idle" || paused);
    this.speed.disabled = this._pending || !status;
    this.root.querySelector(".start-label").textContent = this._pending ? "요청 처리 중…" : paused ? "경기 일시정지" : status === "running" ? "경기 진행 중" : status === "finished" || status === "failed" ? "새 경기 시작" : "경기 시작";
    this.root.querySelector(".pause-label").textContent = paused ? "자동진행" : "일시정지";
    this.root.querySelector(".pause-icon").textContent = paused ? "▶" : "Ⅱ";
    this.root.querySelector(".playback-note").textContent = paused ? "일시정지 · 버튼을 누를 때마다 한 턴 진행합니다." : status === "running" ? "일시정지 후 한 턴씩 진행할 수 있습니다." : status === "finished" || status === "failed" ? "새 경기를 시작해 다시 관전하세요." : "한 턴 진행으로 경기를 시작할 수 있습니다.";
    if (!this._pending && this._snapshot && [...this.speed.options].some(option => Number(option.value) === this._snapshot.turn_delay_ms)) {
      this.speed.value = String(this._snapshot.turn_delay_ms);
    }
  }
}

customElements.define("match-controls", MatchControls);
