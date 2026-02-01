import { BrowserRouter, Routes, Route, Navigate } from "react-router-dom";
import { Dashboard } from "./Dashboard";

function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<Dashboard />} />
        {/* Legacy redirects */}
        <Route path="/admin/*" element={<Navigate to="/" replace />} />
        <Route path="/pair" element={<Navigate to="/" replace />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </BrowserRouter>
  );
}

export default App;
