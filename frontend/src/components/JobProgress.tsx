export interface StatusMessage {
  status: string;
  message?: string;
  timestamp: number;
  eta?: number;
}

interface JobProgressProps {
  messages: StatusMessage[];
  error?: string;
}

function getStatusLabel(status: string): string {
  switch (status) {
    case "processing":
      return "Processing";
    case "success":
      return "Complete";
    case "error":
      return "Error";
    case "payment-required":
      return "Payment Required";
    case "partial":
      return "Partial";
    default:
      return status;
  }
}

function getStatusClass(status: string): string {
  switch (status) {
    case "success":
      return "status-success";
    case "error":
      return "status-error";
    default:
      return "status-processing";
  }
}

export function JobProgress({ messages, error }: JobProgressProps) {
  if (messages.length === 0 && !error) {
    return null;
  }

  return (
    <div className="job-progress">
      <h3>Progress</h3>
      {error && <div className="error-message">{error}</div>}
      <ul className="status-list">
        {messages.map((msg, idx) => (
          <li key={idx} className={getStatusClass(msg.status)}>
            <span className="status-label">{getStatusLabel(msg.status)}</span>
            {msg.message && (
              <span className="status-message">{msg.message}</span>
            )}
          </li>
        ))}
      </ul>
    </div>
  );
}
