import { BrowserRouter, Routes, Route, Navigate } from "react-router-dom";
import { Dashboard } from "./Dashboard";
import { PairDvm } from "./pages/PairDvm";

function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<Dashboard />} />
        {/* Keep pair route temporarily for deep links */}
        <Route path="/pair" element={<PairDvm />} />
        <Route path="/admin/pair" element={<Navigate to="/pair" replace />} />
        <Route path="/admin/*" element={<Navigate to="/" replace />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </BrowserRouter>
  );
}

export default App;
