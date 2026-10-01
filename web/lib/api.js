export class ApiError extends Error {
  constructor(message, status = 0) {
    super(message);
    this.name = "ApiError";
    this.status = status;
  }
}

export async function requestSnapshot(path = "/api/match", { method = "GET", body, signal } = {}) {
  let response;
  try {
    response = await fetch(path, {
      method,
      headers: body ? { "Content-Type": "application/json" } : {},
      body: body ? JSON.stringify(body) : undefined,
      signal,
      cache: "no-store",
    });
  } catch (error) {
    if (error.name === "AbortError") throw error;
    throw new ApiError("서버에 연결할 수 없습니다. 로컬 서버 실행 상태를 확인해 주세요.");
  }
  let result;
  try {
    result = await response.json();
  } catch {
    throw new ApiError("서버의 경기 응답을 읽을 수 없습니다.", response.status);
  }
  if (!response.ok) throw new ApiError(result.error || "경기 요청을 처리하지 못했습니다.", response.status);
  if (!result.state || !["idle", "running", "finished", "failed"].includes(result.status)) {
    throw new ApiError("경기 상태 응답의 형식이 올바르지 않습니다.");
  }
  return result;
}
