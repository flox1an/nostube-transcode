import { BrowserRouter, Routes, Route, Navigate, useLocation } from "react-router-dom";
import { Dashboard } from "./Dashboard";

function RedirectToRoot() {
  const location = useLocation();
  return <Navigate to={`/${location.search}`} replace />;
}

function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<Dashboard />} />
        {/* Legacy redirects */}
        <Route path="/admin/*" element={<RedirectToRoot />} />
        <Route path="*" element={<RedirectToRoot />} />
      </Routes>
    </BrowserRouter>
  );
}

export default App;
