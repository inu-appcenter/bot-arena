import { mountShadow } from "../lib/component.js";
import { renderArena } from "../render/arena-renderer.js";

class ArenaBoard extends HTMLElement {
  constructor() {
    super();
    this.root = mountShadow(this, "/components/arena-board.css", `
      <section aria-labelledby="board-title">
        <div class="board-header"><div class="board-heading"><span class="eyebrow">THE ARENA</span><h2 id="board-title">캠퍼스 경기장</h2></div><span class="map-label">15 × 15 <span>·</span> 고정 연습 지도</span></div>
        <div class="canvas-wrap"><canvas width="900" height="900" role="img" aria-label="경기 상태를 연결하는 중입니다.">지도와 로봇의 실시간 상태는 오른쪽 로봇 현황과 점수판에서도 확인할 수 있습니다.</canvas></div>
        <div class="legend" aria-label="경기장 범례"><span><i class="bank bank-a"></i>A팀 반납 구역</span><span><i class="bank bank-b"></i>B팀 반납 구역</span><span><i class="resource"></i>재활용품</span><span class="legend-coordinate">좌표 (x, y)</span></div>
        <div class="stats"><div><span class="stat-label">경기장에 남은 자원</span><span class="stat-value remaining">—</span><span class="unit">개</span></div><div><span class="stat-label">로봇이 운반 중</span><span class="stat-value carried">—</span><span class="unit">개</span></div><div><span class="stat-label">반납 완료</span><span class="stat-value delivered">—</span><span class="unit total">/ 96개</span></div></div>
      </section>`);
    this.canvas = this.root.querySelector("canvas");
    this._state = null;
    this._signature = "";
    this._resizeObserver = new ResizeObserver(() => renderArena(this.canvas, this._state));
    this._onResize = () => renderArena(this.canvas, this._state);
  }

  connectedCallback() {
    this._resizeObserver.observe(this.canvas);
    window.addEventListener("resize", this._onResize);
    renderArena(this.canvas, this._state);
  }

  disconnectedCallback() {
    this._resizeObserver.disconnect();
    window.removeEventListener("resize", this._onResize);
  }

  set state(state) {
    if (!state) return;
    this._state = state;
    const remaining = state.resources.reduce((sum, value) => sum + value.amount, 0);
    const carried = state.robots.reduce((sum, value) => sum + value.cargo, 0);
    const delivered = state.scores.A + state.scores.B;
    this.root.querySelector(".remaining").textContent = remaining;
    this.root.querySelector(".carried").textContent = carried;
    this.root.querySelector(".delivered").textContent = delivered;
    this.root.querySelector(".total").textContent = `/ ${state.total_resources}개`;
    this.canvas.setAttribute("aria-label", `${state.width} 곱하기 ${state.height} 경기장. ${state.completed_turn}턴 완료. 자원 ${remaining}개 남음. ${state.robots.map(robot => `${robot.id}: (${robot.x}, ${robot.y}), 적재 ${robot.cargo}개`).join(". ")}`);
    const signature = JSON.stringify(state);
    if (signature !== this._signature) { this._signature = signature; renderArena(this.canvas, state); }
  }
}

customElements.define("arena-board", ArenaBoard);
