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
        <button class="restart" type="button"><span aria-hidden="true">↻</span> 새 경기로 재시작</button>
        <div class="speed-row"><label for="speed">관전 속도</label><select id="speed" aria-label="관전 속도"><option value="450">천천히 · 0.45초/턴</option><option value="150" selected>보통 · 0.15초/턴</option><option value="40">빠르게 · 0.04초/턴</option></select></div>
        <p class="speed-note">속도는 경기의 판정 결과에 영향을 주지 않습니다.</p>
      </section>`);
    this.start = this.root.querySelector(".start");
    this.restart = this.root.querySelector(".restart");
    this.speed = this.root.querySelector("select");
    this._snapshot = null;
    this._pending = false;
    this._onStart = () => emit(this, "match-start", { turnDelay: Number(this.speed.value) });
    this._onRestart = () => emit(this, "match-restart", { turnDelay: Number(this.speed.value) });
    this._onSpeed = () => emit(this, "match-speed", { turnDelay: Number(this.speed.value) });
  }

  connectedCallback() {
    this.start.addEventListener("click", this._onStart);
    this.restart.addEventListener("click", this._onRestart);
    this.speed.addEventListener("change", this._onSpeed);
    this.render();
  }

  disconnectedCallback() {
    this.start.removeEventListener("click", this._onStart);
    this.restart.removeEventListener("click", this._onRestart);
    this.speed.removeEventListener("change", this._onSpeed);
  }

  set snapshot(value) { this._snapshot = value; this.render(); }
  set pending(value) { this._pending = value; this.render(); }

  render() {
    const status = this._snapshot?.status;
    this.start.disabled = this._pending || !status || status === "running";
    this.restart.disabled = this._pending || !status || status === "idle";
    this.speed.disabled = this._pending || !status;
    this.root.querySelector(".start-label").textContent = this._pending ? "경기 연결 중…" : status === "running" ? "경기 진행 중" : status === "finished" || status === "failed" ? "새 경기 시작" : "경기 시작";
    if (!this._pending && this._snapshot && [...this.speed.options].some(option => Number(option.value) === this._snapshot.turn_delay_ms)) {
      this.speed.value = String(this._snapshot.turn_delay_ms);
    }
  }
}

customElements.define("match-controls", MatchControls);
